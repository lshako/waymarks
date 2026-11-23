//! Reading the local Garmin store that `garmin sync` fills.
//!
//! The store keeps two sets of Parquet files: one row per activity under
//! `activities/`, and the points of every recorded track under
//! `track_points/`. Everything the map needs is in there, so importing never
//! talks to Garmin itself — `garmin sync` does that, and keeps track of what
//! it has already downloaded.

use crate::config::Config;
use anyhow::{Context, Result, bail};
use chrono::{DateTime, NaiveDate};
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::{Field, Row};
use parquet::schema::types::Type;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

/// One recorded activity, with only the columns the map cares about.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Activity {
    pub id: i64,
    pub name: String,
    pub activity_type: String,
    pub date: Option<NaiveDate>,
    /// Where the activity started, as `(lat, lon)`.
    pub start: Option<(f64, f64)>,
}

/// The highest point of a recorded track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Peak {
    pub lat: f64,
    pub lon: f64,
    pub elevation: f64,
}

/// Runs `garmin sync` so the store holds everything up to today.
///
/// The tool keeps its own record of what it has downloaded, so a later run
/// only fetches what is new; `backfill` is what reaches back through the
/// whole history the first time.
pub(crate) fn sync(config: &Config, backfill: bool) -> Result<()> {
    let store = config.garmin_store();
    let mut command = Command::new(&config.garmin.command);
    command.arg("sync").arg("run").arg("--activities");
    command.arg("--storage").arg(&store);
    if backfill {
        command.arg("--backfill");
    }

    println!(
        "Running {} sync{}...",
        config.garmin.command,
        if backfill { " (full history)" } else { "" }
    );

    let status = command.status().with_context(|| {
        format!(
            "Failed to run '{}'. Install it with `cargo install garmin-cli` \
             and log in with `{} auth login`",
            config.garmin.command, config.garmin.command
        )
    })?;

    if !status.success() {
        bail!(
            "'{} sync run' failed ({status}). Check `{} auth status`",
            config.garmin.command,
            config.garmin.command
        );
    }

    Ok(())
}

/// Whether the store holds no activities yet, i.e. the next sync has to reach
/// back through the whole history.
pub(crate) fn is_empty(store: &Path) -> bool {
    parquet_files(&store.join("activities")).is_none_or(|files| files.is_empty())
}

/// Every activity in the store, newest first.
pub(crate) fn read_activities(store: &Path) -> Result<Vec<Activity>> {
    let dir = store.join("activities");
    let files = parquet_files(&dir)
        .with_context(|| format!("No activities in {}. Run a sync first.", store.display()))?;

    let columns = [
        "activity_id",
        "activity_name",
        "activity_type",
        "start_time_local",
        "start_lat",
        "start_lon",
    ];

    let mut activities = Vec::new();

    for file in files {
        for_each_row(&file, &columns, |row| {
            let fields = named(row);

            let Some(id) = fields.get("activity_id").and_then(|f| as_i64(f)) else {
                return;
            };

            activities.push(Activity {
                id,
                name: fields
                    .get("activity_name")
                    .and_then(|f| as_string(f))
                    .unwrap_or_else(|| format!("activity {id}")),
                activity_type: fields
                    .get("activity_type")
                    .and_then(|f| as_string(f))
                    .unwrap_or_default(),
                date: fields.get("start_time_local").and_then(|f| as_date(f)),
                start: match (
                    fields.get("start_lat").and_then(|f| as_f64(f)),
                    fields.get("start_lon").and_then(|f| as_f64(f)),
                ) {
                    (Some(lat), Some(lon)) => Some((lat, lon)),
                    _ => None,
                },
            });
        })?;
    }

    activities.sort_by_key(|activity| std::cmp::Reverse(activity.date));
    Ok(activities)
}

/// The highest point of each of the given activities, walking the track
/// points a file at a time so a long history never has to be held in memory.
pub(crate) fn read_peaks(store: &Path, wanted: &HashSet<i64>) -> Result<HashMap<i64, Peak>> {
    let mut peaks: HashMap<i64, Peak> = HashMap::new();
    if wanted.is_empty() {
        return Ok(peaks);
    }

    let dir = store.join("track_points");
    let Some(files) = parquet_files(&dir) else {
        return Ok(peaks);
    };

    let columns = ["activity_id", "lat", "lon", "elevation"];

    for file in files {
        for_each_row(&file, &columns, |row| {
            let fields = named(row);

            let Some(id) = fields.get("activity_id").and_then(|f| as_i64(f)) else {
                return;
            };
            if !wanted.contains(&id) {
                return;
            }

            let (Some(lat), Some(lon), Some(elevation)) = (
                fields.get("lat").and_then(|f| as_f64(f)),
                fields.get("lon").and_then(|f| as_f64(f)),
                fields.get("elevation").and_then(|f| as_f64(f)),
            ) else {
                return;
            };

            let peak = Peak {
                lat,
                lon,
                elevation,
            };

            peaks
                .entry(id)
                .and_modify(|highest| {
                    if elevation > highest.elevation {
                        *highest = peak;
                    }
                })
                .or_insert(peak);
        })?;
    }

    Ok(peaks)
}

/// The Parquet files of a directory, sorted so runs are reproducible.
fn parquet_files(dir: &Path) -> Option<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "parquet"))
        .collect();

    files.sort();
    Some(files)
}

/// Reads a Parquet file row by row, pulling only the wanted columns — the
/// activity rows carry the whole Garmin response in `raw_json`, which is
/// worth skipping — and handing each row on without holding the file in
/// memory.
fn for_each_row(path: &Path, columns: &[&str], mut visit: impl FnMut(&Row)) -> Result<()> {
    let file =
        fs::File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
    let reader = SerializedFileReader::new(file)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let schema = reader.metadata().file_metadata().schema();
    let fields: Vec<Arc<Type>> = schema
        .get_fields()
        .iter()
        .filter(|field| columns.contains(&field.name()))
        .cloned()
        .collect();

    // The projection is checked against the root group by name too, and the
    // files are written by arrow, whose root is not called "schema".
    let projection = Type::group_type_builder(schema.name())
        .with_fields(fields)
        .build()
        .context("Failed to build the column projection")?;

    let rows = reader
        .get_row_iter(Some(projection))
        .with_context(|| format!("Failed to read rows of {}", path.display()))?;

    for row in rows {
        let row = row.with_context(|| format!("Failed to read a row of {}", path.display()))?;
        visit(&row);
    }

    Ok(())
}

fn named(row: &Row) -> HashMap<&str, &Field> {
    row.get_column_iter()
        .map(|(name, field)| (name.as_str(), field))
        .collect()
}

fn as_i64(field: &Field) -> Option<i64> {
    match field {
        Field::Long(value) => Some(*value),
        Field::Int(value) => Some(*value as i64),
        _ => None,
    }
}

fn as_f64(field: &Field) -> Option<f64> {
    match field {
        Field::Double(value) => Some(*value),
        Field::Float(value) => Some(*value as f64),
        _ => None,
    }
}

fn as_string(field: &Field) -> Option<String> {
    match field {
        Field::Str(value) if !value.is_empty() => Some(value.clone()),
        _ => None,
    }
}

/// The day a timestamp column falls on. Garmin's local start time is stored
/// as a timestamp, so the date is read off it directly.
fn as_date(field: &Field) -> Option<NaiveDate> {
    let millis = match field {
        Field::TimestampMillis(value) => *value,
        Field::TimestampMicros(value) => value / 1_000,
        Field::Long(value) => *value,
        _ => return None,
    };

    DateTime::from_timestamp_millis(millis).map(|stamp| stamp.date_naive())
}

#[cfg(test)]
mod tests {
    use super::*;
    use parquet::data_type::{ByteArray, ByteArrayType, DoubleType, Int64Type};
    use parquet::file::properties::WriterProperties;
    use parquet::file::writer::SerializedFileWriter;
    use std::sync::Arc;

    /// Writes a Parquet file shaped like the ones `garmin sync` produces.
    fn write(path: &Path, schema: &str, columns: Vec<Column>) {
        let schema = Arc::new(parquet::schema::parser::parse_message_type(schema).unwrap());
        let file = fs::File::create(path).unwrap();
        let mut writer =
            SerializedFileWriter::new(file, schema, Arc::new(WriterProperties::new())).unwrap();

        let mut group = writer.next_row_group().unwrap();
        let mut index = 0;
        while let Some(mut column) = group.next_column().unwrap() {
            match &columns[index] {
                Column::Long(values) => column
                    .typed::<Int64Type>()
                    .write_batch(values, None, None)
                    .unwrap(),
                Column::Double(values) => column
                    .typed::<DoubleType>()
                    .write_batch(values, None, None)
                    .unwrap(),
                Column::Text(values) => {
                    let values: Vec<ByteArray> =
                        values.iter().map(|v| ByteArray::from(v.as_str())).collect();
                    column
                        .typed::<ByteArrayType>()
                        .write_batch(&values, None, None)
                        .unwrap()
                }
            };
            column.close().unwrap();
            index += 1;
        }
        group.close().unwrap();
        writer.close().unwrap();
    }

    enum Column {
        Long(Vec<i64>),
        Double(Vec<f64>),
        Text(Vec<String>),
    }

    fn store_with_two_activities() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("activities")).unwrap();
        fs::create_dir_all(dir.path().join("track_points")).unwrap();

        write(
            &dir.path().join("activities/2026-W32.parquet"),
            "message arrow_schema {
                REQUIRED INT64 activity_id;
                REQUIRED BYTE_ARRAY activity_name (UTF8);
                REQUIRED BYTE_ARRAY activity_type (UTF8);
                REQUIRED INT64 start_time_local (TIMESTAMP_MILLIS);
                REQUIRED DOUBLE start_lat;
                REQUIRED DOUBLE start_lon;
                REQUIRED BYTE_ARRAY raw_json (UTF8);
            }",
            vec![
                Column::Long(vec![1, 2]),
                Column::Text(vec!["Grindelwald Hiking".into(), "Lunch Ride".into()]),
                Column::Text(vec!["hiking".into(), "cycling".into()]),
                // Local start times: 2026-08-09T05:26:49 and 2026-08-08T13:40:21.
                // Garmin stores the local wall clock, so the day is read off
                // the timestamp as it stands.
                Column::Long(vec![1_786_253_209_000, 1_786_196_421_000]),
                Column::Double(vec![46.583446, 52.52]),
                Column::Double(vec![8.022450, 13.405]),
                Column::Text(vec!["{\"big\": true}".into(), "{}".into()]),
            ],
        );

        write(
            &dir.path().join("track_points/2026-08-09.parquet"),
            "message arrow_schema {
                REQUIRED INT64 activity_id;
                REQUIRED DOUBLE lat;
                REQUIRED DOUBLE lon;
                REQUIRED DOUBLE elevation;
            }",
            vec![
                Column::Long(vec![1, 1, 1, 2]),
                Column::Double(vec![46.5834, 46.6000, 46.5900, 52.52]),
                Column::Double(vec![8.0224, 8.0500, 8.0400, 13.405]),
                Column::Double(vec![3374.6, 3954.6, 3600.0, 34.0]),
            ],
        );

        dir
    }

    #[test]
    fn reads_the_activities_of_the_store() {
        let store = store_with_two_activities();
        let activities = read_activities(store.path()).unwrap();

        assert_eq!(activities.len(), 2);
        // Newest first.
        let hike = &activities[0];
        assert_eq!(hike.id, 1);
        assert_eq!(hike.name, "Grindelwald Hiking");
        assert_eq!(hike.activity_type, "hiking");
        assert_eq!(hike.date, NaiveDate::from_ymd_opt(2026, 8, 9));
        let (lat, lon) = hike.start.unwrap();
        assert!((lat - 46.583446).abs() < 1e-6 && (lon - 8.022450).abs() < 1e-6);

        assert_eq!(activities[1].activity_type, "cycling");
    }

    #[test]
    fn finds_the_highest_point_of_each_wanted_activity() {
        let store = store_with_two_activities();
        let peaks = read_peaks(store.path(), &HashSet::from([1])).unwrap();

        assert_eq!(peaks.len(), 1, "only the wanted activity is measured");
        let peak = peaks[&1];
        assert_eq!(peak.elevation, 3954.6);
        assert!((peak.lat - 46.6).abs() < 1e-6, "peak at the highest point");
        assert!((peak.lon - 8.05).abs() < 1e-6);
    }

    #[test]
    fn measures_nothing_when_no_activity_is_wanted() {
        let store = store_with_two_activities();
        assert!(
            read_peaks(store.path(), &HashSet::new())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn an_empty_store_is_recognised() {
        let empty = tempfile::tempdir().unwrap();
        assert!(is_empty(empty.path()));
        assert!(read_activities(empty.path()).is_err());

        let filled = store_with_two_activities();
        assert!(!is_empty(filled.path()));
    }
}
