//! The summits of `docs/summits.json`, stored as a GeoJSON feature
//! collection so the map can load the file as it is.

use crate::geo::haversine_meters;
use anyhow::Result;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Two summits this close together are taken to be the same one, even when
/// the activities that recorded them were named differently.
const SAME_SUMMIT_METERS: f64 = 150.0;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Summits {
    #[serde(rename = "type", default = "feature_collection")]
    kind: String,
    features: Vec<Feature>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Feature {
    #[serde(rename = "type", default = "feature")]
    kind: String,
    pub properties: Properties,
    pub geometry: Geometry,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Properties {
    pub name: String,
    pub elevation: i32,
    pub date: NaiveDate,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Geometry {
    #[serde(rename = "type", default = "point")]
    kind: String,
    /// GeoJSON order: longitude first, then latitude.
    pub coordinates: [f64; 2],
}

impl Geometry {
    pub fn lon(&self) -> f64 {
        self.coordinates[0]
    }

    pub fn lat(&self) -> f64 {
        self.coordinates[1]
    }
}

/// A summit as the rest of the crate deals with it, before it becomes a
/// GeoJSON feature.
#[derive(Debug, Clone, PartialEq)]
pub struct Summit {
    pub name: String,
    pub elevation: i32,
    pub date: NaiveDate,
    pub lat: f64,
    pub lon: f64,
}

impl Summits {
    pub fn new() -> Self {
        Self {
            kind: feature_collection(),
            features: Vec::new(),
        }
    }

    pub fn features(&self) -> &[Feature] {
        &self.features
    }

    /// Whether a summit of that name was already visited, ignoring case and
    /// surrounding whitespace.
    ///
    /// Names do not identify a summit — Garmin names an activity after the
    /// nearest place, so every hike from the same valley carries the same one
    /// — which is why [`Summits::add`] goes by position instead.
    #[allow(dead_code)]
    pub fn contains_name(&self, name: &str) -> bool {
        let name = name.trim();
        self.features
            .iter()
            .any(|f| f.properties.name.trim().eq_ignore_ascii_case(name))
    }

    /// Adds a summit unless a known one stands within [`SAME_SUMMIT_METERS`]
    /// of it. Returns whether it was added.
    ///
    /// Position is what tells two summits apart: importing the same activity
    /// again lands on the same peak and changes nothing, while two climbs
    /// that happen to share a name are kept apart.
    pub fn add(&mut self, summit: Summit) -> bool {
        let known_peak = self.features.iter().any(|f| {
            haversine_meters(summit.lat, summit.lon, f.geometry.lat(), f.geometry.lon())
                <= SAME_SUMMIT_METERS
        });
        if known_peak {
            return false;
        }

        self.features.push(Feature {
            kind: feature(),
            properties: Properties {
                name: summit.name,
                elevation: summit.elevation,
                date: summit.date,
            },
            geometry: Geometry {
                kind: point(),
                coordinates: [summit.lon, summit.lat],
            },
        });
        true
    }

    /// Gives the summit standing at that position a better name, and reports
    /// whether it changed.
    ///
    /// Summits first land under the name of the activity that recorded them;
    /// this is how they take the name of the mountain once it is known.
    pub fn rename_at(&mut self, lat: f64, lon: f64, name: &str) -> bool {
        let Some(summit) = self.features.iter_mut().find(|f| {
            haversine_meters(lat, lon, f.geometry.lat(), f.geometry.lon()) <= SAME_SUMMIT_METERS
        }) else {
            return false;
        };

        if summit.properties.name == name {
            return false;
        }

        summit.properties.name = name.to_string();
        true
    }

    /// Writes the collection out, oldest summit first, so repeated syncs
    /// produce a readable diff.
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let mut sorted = self.clone();
        sorted.features.sort_by(|a, b| {
            (a.properties.date, &a.properties.name).cmp(&(b.properties.date, &b.properties.name))
        });

        let json = serde_json::to_string_pretty(&sorted)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let summits: Summits = serde_json::from_str(&content)?;
        Ok(summits)
    }

    /// Loads the file, starting from an empty collection when it is missing
    /// or unreadable.
    pub fn load_or_new(path: impl AsRef<Path>) -> Self {
        Self::load_from_file(path).unwrap_or_else(|_| Self::new())
    }
}

impl Default for Summits {
    fn default() -> Self {
        Self::new()
    }
}

fn feature_collection() -> String {
    "FeatureCollection".to_string()
}

fn feature() -> String {
    "Feature".to_string()
}

fn point() -> String {
    "Point".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summit(name: &str, lat: f64, lon: f64) -> Summit {
        Summit {
            name: name.to_string(),
            elevation: 2962,
            date: NaiveDate::from_ymd_opt(2023, 8, 15).unwrap(),
            lat,
            lon,
        }
    }

    #[test]
    fn add_rejects_the_same_peak_twice() {
        let mut summits = Summits::new();

        assert!(summits.add(summit("Zugspitze", 47.4211, 10.9863)));
        // The same peak under another name, e.g. a re-import after a rename.
        assert!(!summits.add(summit("Zugspitze via Höllental", 47.42115, 10.98635)));
        // A genuinely different summit.
        assert!(summits.add(summit("Watzmann", 47.5551, 12.9224)));

        assert_eq!(summits.features().len(), 2);
    }

    #[test]
    fn two_summits_may_share_a_name() {
        let mut summits = Summits::new();

        // Garmin names every hike after the valley it starts in, so two
        // different peaks above Grindelwald arrive under one name.
        assert!(summits.add(summit("Grindelwald Hiking", 46.5833, 8.0226)));
        assert!(summits.add(summit("Grindelwald Hiking", 46.6000, 8.0500)));

        assert_eq!(summits.features().len(), 2);
        assert!(summits.contains_name("grindelwald hiking"));
    }

    #[test]
    fn round_trips_as_geojson() -> Result<()> {
        let mut summits = Summits::new();
        summits.add(Summit {
            name: "Watzmann".to_string(),
            elevation: 2713,
            date: NaiveDate::from_ymd_opt(2024, 7, 1).unwrap(),
            lat: 47.5551,
            lon: 12.9224,
        });
        summits.add(summit("Zugspitze", 47.4211, 10.9863));

        let mut path = std::env::temp_dir();
        path.push("waymarks_summits_test.json");
        summits.save_to_file(&path)?;

        let raw = fs::read_to_string(&path)?;
        assert!(raw.contains(r#""type": "FeatureCollection""#));
        assert!(raw.contains(r#""date": "2023-08-15""#));
        // Saved oldest first, whatever the order they were added in.
        assert!(raw.find("Zugspitze").unwrap() < raw.find("Watzmann").unwrap());

        let loaded = Summits::load_from_file(&path)?;
        assert_eq!(loaded.features().len(), 2);
        let zugspitze = &loaded.features()[0];
        assert_eq!(zugspitze.properties.name, "Zugspitze");
        assert_eq!(zugspitze.properties.elevation, 2962);
        assert_eq!(zugspitze.geometry.lat(), 47.4211);
        assert_eq!(zugspitze.geometry.lon(), 10.9863);

        fs::remove_file(&path)?;
        Ok(())
    }

    #[test]
    fn reads_a_collection_written_by_hand() -> Result<()> {
        let raw = r#"{
          "type": "FeatureCollection",
          "features": [
            {
              "type": "Feature",
              "properties": { "name": "Test", "elevation": 4478, "date": "2023-08-15" },
              "geometry": { "type": "Point", "coordinates": [7.6586, 45.9763] }
            }
          ]
        }"#;

        let summits: Summits = serde_json::from_str(raw)?;
        assert!(summits.contains_name("test"));
        assert_eq!(summits.features()[0].geometry.lon(), 7.6586);
        Ok(())
    }

    #[test]
    fn load_or_new_starts_empty_when_the_file_is_missing() {
        let summits = Summits::load_or_new("/nonexistent/summits.json");
        assert!(summits.features().is_empty());
    }
}
