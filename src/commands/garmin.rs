//! The `import-garmin` command: turns the locally synced Garmin activities
//! into the summits, cities and countries the map draws.
//!
//! Downloading is left to `garmin sync`, which knows what it already has, so
//! the first run fetches the whole history and later ones only what is new.
//! Importing is then a local read that adds whatever is missing — running it
//! twice changes nothing the second time.

use crate::config::Config;
use crate::garmin::{self, Activity, Peak};
use crate::geo::GeonameIndex;
use crate::geonames::{self, CountryMaps, normalize_country_name};
use crate::models::boundaries::CountryBoundaries;
use crate::models::cities::{Cities, Coordinates};
use crate::models::countries::Countries;
use crate::models::summits::{Summit, Summits};
use anyhow::Result;
use colored::Colorize;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, Copy)]
pub(crate) struct ImportOptions {
    /// Run `garmin sync` first, so the store is up to date.
    pub sync: bool,
    /// Reach back through the whole history, even when the store already
    /// holds activities.
    pub backfill: bool,
    /// Work everything out but leave the files untouched.
    pub dry_run: bool,
}

pub(crate) async fn import(config: &Config, options: ImportOptions) -> Result<()> {
    let store = config.garmin_store();

    if options.sync {
        // An empty store has a whole history to fetch, whatever was asked.
        let backfill = options.backfill || garmin::is_empty(&store);
        garmin::sync(config, backfill)?;
    }

    let activities = garmin::read_activities(&store)?;
    println!(
        "{}",
        format!("{} activities in {}", activities.len(), store.display()).blue()
    );

    Importer::new(config)
        .await?
        .run(&activities, &store, options)
        .await
}

/// A named peak from the Geonames dump, standing at its own surveyed
/// position rather than wherever a track happened to record it.
#[derive(Debug, Clone)]
struct Mountain {
    name: String,
    lat: f64,
    lon: f64,
}

/// The map data being added to, and everything needed to place a coordinate.
struct Importer<'a> {
    config: &'a Config,
    boundaries: CountryBoundaries,
    cities_index: GeonameIndex,
    country_maps: CountryMaps,
    countries: Countries,
    summits: Summits,
    /// The city files being filled, keyed by country name.
    cities: BTreeMap<String, Cities>,
    added_summits: usize,
    renamed_summits: usize,
    added_cities: usize,
    added_countries: usize,
    unplaced: usize,
    /// High points with no named mountain near them, left off the map.
    unnamed_peaks: usize,
}

impl<'a> Importer<'a> {
    async fn new(config: &'a Config) -> Result<Self> {
        let boundaries = CountryBoundaries::load_from_dir(config.countries_geo_dir())?;
        println!("Loaded {} country outlines", boundaries.len());

        let country_maps =
            CountryMaps::from_country_info(&geonames::load_country_info(config).await?);
        let cities_index = GeonameIndex::build(geonames::load_cities(config).await?);

        Ok(Self {
            config,
            boundaries,
            cities_index,
            country_maps,
            countries: Countries::load_from_file(config.countries_path())?,
            summits: Summits::load_or_new(config.summits_path()),
            cities: BTreeMap::new(),
            added_summits: 0,
            renamed_summits: 0,
            added_cities: 0,
            added_countries: 0,
            unplaced: 0,
            unnamed_peaks: 0,
        })
    }

    async fn run(
        mut self,
        activities: &[Activity],
        store: &std::path::Path,
        options: ImportOptions,
    ) -> Result<()> {
        // Only mountain activities are worth walking the track points for.
        let candidates: Vec<&Activity> = activities
            .iter()
            .filter(|activity| self.is_summit_candidate(activity))
            .collect();

        println!("{} of them could have reached a summit", candidates.len());

        let wanted: HashSet<i64> = candidates.iter().map(|activity| activity.id).collect();
        let peaks = garmin::read_peaks(store, &wanted)?;

        let names = self.summit_names(&peaks).await?;

        // Oldest first, so a summit is dated the first time it was climbed
        // and a place is added under the country it was first seen in.
        let mut in_order: Vec<&Activity> = activities.iter().collect();
        in_order.sort_by_key(|activity| activity.date);

        for activity in in_order {
            self.place(activity);

            if let Some(peak) = peaks.get(&activity.id) {
                self.add_summit(activity, *peak, names.get(&activity.id).cloned());
            }
        }

        self.save(options.dry_run)?;
        self.report(options.dry_run);
        Ok(())
    }

    /// Looks up the name of the mountain each peak stands on.
    ///
    /// The named peaks live in the Geonames dump of their own country, so the
    /// peaks are grouped by country and only the dumps actually needed are
    /// downloaded.
    async fn summit_names(&self, peaks: &HashMap<i64, Peak>) -> Result<HashMap<i64, Mountain>> {
        let mut by_country: BTreeMap<String, Vec<(i64, Peak)>> = BTreeMap::new();

        for (id, peak) in peaks {
            if peak.elevation < self.config.garmin.min_elevation {
                continue;
            }
            if let Some(iso) = self.iso_of(peak.lat, peak.lon) {
                by_country.entry(iso).or_default().push((*id, *peak));
            }
        }

        let mut names = HashMap::new();
        let radius_km = self.config.garmin.summit_name_radius_m / 1000.0;

        for (iso, peaks) in by_country {
            let features = geonames::load_summit_features(
                self.config,
                &iso,
                &self.config.garmin.summit_feature_codes,
            )
            .await?;

            println!(
                "{}",
                format!("{} named peaks in {}", features.len(), iso.to_uppercase()).blue()
            );

            let mountains = GeonameIndex::build(features);

            let tolerance = self.config.garmin.summit_name_elevation_tolerance_m;

            for (id, peak) in peaks {
                // A peak of a known, very different height is a neighbour
                // rather than the one that was climbed. Only the surveyed
                // height counts: the `dem` fallback samples a terrain model
                // that smooths summits away, and a watch reading barometric
                // altitude drifts by a couple of hundred meters, so the
                // tolerance is wide and the check is skipped where Geonames
                // has no surveyed height.
                let fits = |mountain: &crate::geonames::Geoname| {
                    mountain
                        .elevation
                        .map(f64::from)
                        .filter(|height| *height > 0.0)
                        .is_none_or(|height| (height - peak.elevation).abs() <= tolerance)
                };

                if let Some((mountain, _)) =
                    mountains.nearest_matching(peak.lat, peak.lon, radius_km, fits)
                {
                    names.insert(
                        id,
                        Mountain {
                            name: mountain.name.clone(),
                            lat: mountain.latitude,
                            lon: mountain.longitude,
                        },
                    );
                }
            }
        }

        Ok(names)
    }

    /// The ISO code of the country a coordinate falls in, needed to pick the
    /// right Geonames dump.
    fn iso_of(&self, lat: f64, lon: f64) -> Option<String> {
        let from_outline = self
            .boundaries
            .country_of(lat, lon)
            // The outlines name countries as the map files do; Geonames knows
            // them with spaces.
            .and_then(|name| self.country_maps.get_iso(&name.replace('_', " ")))
            .cloned();

        from_outline.or_else(|| {
            self.cities_index
                .nearest(lat, lon, self.config.garmin.max_city_distance_km)
                .and_then(|(city, _)| city.country_code.clone())
                .map(|iso| iso.to_lowercase())
        })
    }

    /// Whether an activity is the kind that reaches summits. An empty list of
    /// types in the config accepts every activity.
    fn is_summit_candidate(&self, activity: &Activity) -> bool {
        let types = &self.config.garmin.activity_types;

        types.is_empty()
            || types
                .iter()
                .any(|wanted| wanted.eq_ignore_ascii_case(&activity.activity_type))
    }

    /// Records the country and the closest city of where an activity started.
    fn place(&mut self, activity: &Activity) {
        let Some((lat, lon)) = activity.start else {
            return;
        };

        // The outlines are simplified, so a start on a coast can fall outside
        // all of them; the closest city then decides.
        let nearest = self
            .cities_index
            .nearest(lat, lon, self.config.garmin.max_city_distance_km)
            .map(|(city, distance)| {
                // The outlines name the countries the map can draw, so a city
                // is filed under the country its own position falls in.
                // Geonames only fills in for cities outside every outline, and
                // names them differently ("palestinian territory" against the
                // outline's "palestine").
                let country = self
                    .boundaries
                    .country_of(city.latitude, city.longitude)
                    .map(normalize_country_name)
                    .or_else(|| {
                        city.country_code
                            .as_deref()
                            .and_then(|iso| self.country_maps.get_name(iso))
                            .map(|name| normalize_country_name(name))
                    });

                (
                    city.name.clone(),
                    Coordinates {
                        lat: city.latitude,
                        lon: city.longitude,
                    },
                    country,
                    distance,
                )
            });

        let from_outline = self
            .boundaries
            .country_of(lat, lon)
            .map(normalize_country_name);
        let from_city = nearest
            .as_ref()
            .and_then(|(_, _, country, _)| country.clone());

        let Some(country) = from_outline.or_else(|| from_city.clone()) else {
            self.unplaced += 1;
            return;
        };

        self.add_country(&country);

        // A city belongs in the file of its own country, which near a border
        // is not always the country the activity started in.
        let Some((name, coordinates, Some(city_country), distance)) = nearest else {
            return;
        };
        if distance > self.config.garmin.city_radius_km * 1000.0 {
            return;
        }

        self.add_country(&city_country);

        let path = self.config.cities_path(&city_country);
        let file = self
            .cities
            .entry(city_country.clone())
            .or_insert_with(|| Cities::load_from_file(path).unwrap_or_else(|_| Cities::new()));

        if file.add(name.clone(), coordinates) {
            self.added_cities += 1;
            println!("{}", format!("Added city: {name} ({city_country})").green());
        }
    }

    fn add_country(&mut self, country: &str) {
        if self.countries.add(country) {
            self.added_countries += 1;
            println!("{}", format!("Added country: {country}").green());
        }
    }

    /// Records the peak of an activity as the mountain it stands on.
    ///
    /// A high point with no named mountain near it is not a summit worth
    /// putting on the map, so it is counted and dropped. The mountain also
    /// decides where the summit goes: standing it on the surveyed peak rather
    /// than on the point the watch happened to record means two climbs of the
    /// same mountain land on the same spot, and count as one summit.
    fn add_summit(&mut self, activity: &Activity, peak: Peak, mountain: Option<Mountain>) {
        if peak.elevation < self.config.garmin.min_elevation {
            return;
        }

        let Some(date) = activity.date else {
            return;
        };

        let Some(mountain) = mountain else {
            self.unnamed_peaks += 1;
            return;
        };

        let Mountain { name, lat, lon } = mountain;
        let elevation = peak.elevation.round();

        let added = self.summits.add(Summit {
            name: name.clone(),
            elevation: elevation as i32,
            date,
            lat,
            lon,
        });

        if added {
            self.added_summits += 1;
            println!(
                "{}",
                format!(
                    "Added summit: {name} ({elevation} m, {date}) from '{}'",
                    activity.name
                )
                .green()
            );
            return;
        }

        // Already on the map, possibly under an older name.
        if self.summits.rename_at(lat, lon, &name) {
            self.renamed_summits += 1;
            println!("{}", format!("Renamed summit to: {name}").green());
        }
    }

    fn save(&self, dry_run: bool) -> Result<()> {
        if dry_run {
            return Ok(());
        }

        if self.added_countries > 0 {
            self.countries.save_to_file(self.config.countries_path())?;
        }

        if self.added_summits > 0 || self.renamed_summits > 0 {
            self.summits.save_to_file(self.config.summits_path())?;
        }

        if self.added_cities > 0 {
            for (country, cities) in &self.cities {
                cities.save_to_file(self.config.cities_path(country))?;
            }
        }

        Ok(())
    }

    fn report(&self, dry_run: bool) {
        println!();

        if self.unnamed_peaks > 0 {
            println!(
                "{}",
                format!(
                    "{} peaks had no named mountain within {} m and were skipped",
                    self.unnamed_peaks, self.config.garmin.summit_name_radius_m
                )
                .yellow()
            );
        }

        if self.unplaced > 0 {
            let plural = if self.unplaced == 1 {
                "activity"
            } else {
                "activities"
            };
            println!(
                "{}",
                format!(
                    "{} {plural} could not be placed in a country",
                    self.unplaced
                )
                .yellow()
            );
        }

        let mut summary = format!(
            "Added {} summits, {} cities, {} countries ({} summits on the map)",
            self.added_summits,
            self.added_cities,
            self.added_countries,
            self.summits.features().len()
        );

        if self.renamed_summits > 0 {
            summary = format!("{summary}, {} renamed", self.renamed_summits);
        }

        if dry_run {
            println!("{}", format!("{summary} (dry run, nothing written)").blue());
        } else {
            println!("{}", summary.blue());
        }
    }
}
