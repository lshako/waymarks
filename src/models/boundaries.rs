//! The country outlines of `docs/countries-geo`, used to tell which country a
//! coordinate falls in.
//!
//! The map draws a visited country by loading `countries-geo/<name>.json`, so
//! the file names are also the names written to `countries.json`.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct FeatureCollection {
    features: Vec<Feature>,
}

#[derive(Debug, Deserialize)]
struct Feature {
    geometry: Geometry,
}

/// Only the geometries the outlines actually use.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum Geometry {
    /// Rings of `[lon, lat]` pairs: the outline first, then any holes.
    Polygon {
        coordinates: Vec<Ring>,
    },
    MultiPolygon {
        coordinates: Vec<Vec<Ring>>,
    },
}

type Ring = Vec<[f64; 2]>;

/// One country outline, with the box around it that rules out most lookups
/// before any ring is walked.
#[derive(Debug)]
struct Outline {
    rings: Vec<Ring>,
    min_lat: f64,
    max_lat: f64,
    min_lon: f64,
    max_lon: f64,
}

#[derive(Debug)]
struct Country {
    name: String,
    outlines: Vec<Outline>,
}

/// Every country outline shipped in `docs/countries-geo`.
#[derive(Debug)]
pub struct CountryBoundaries {
    countries: Vec<Country>,
}

impl CountryBoundaries {
    /// Reads every `*.json` of the directory, taking the file stem as the
    /// country name.
    pub fn load_from_dir(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref();
        let entries = fs::read_dir(dir)
            .with_context(|| format!("Failed to read country outlines in {}", dir.display()))?;

        let mut countries = Vec::new();

        for entry in entries {
            let path = entry?.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };

            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
            let collection: FeatureCollection = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse {}", path.display()))?;

            countries.push(Country {
                name: name.to_string(),
                outlines: collection
                    .features
                    .into_iter()
                    .flat_map(|feature| feature.geometry.into_rings())
                    .map(Outline::new)
                    .collect(),
            });
        }

        countries.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(Self { countries })
    }

    /// How many countries have an outline.
    pub fn len(&self) -> usize {
        self.countries.len()
    }

    /// The country a coordinate falls in, named as the map expects it.
    ///
    /// The outlines are simplified, so a point right on a coast or a border
    /// may fall outside all of them and come back as `None`.
    pub fn country_of(&self, lat: f64, lon: f64) -> Option<&str> {
        self.countries
            .iter()
            .find(|country| {
                country
                    .outlines
                    .iter()
                    .any(|outline| outline.contains(lat, lon))
            })
            .map(|country| country.name.as_str())
    }
}

impl Geometry {
    /// Splits a geometry into the ring sets of the polygons it holds.
    fn into_rings(self) -> Vec<Vec<Ring>> {
        match self {
            Geometry::Polygon { coordinates } => vec![coordinates],
            Geometry::MultiPolygon { coordinates } => coordinates,
        }
    }
}

impl Outline {
    fn new(rings: Vec<Ring>) -> Self {
        let points = rings.iter().flatten();

        let mut min_lat = f64::MAX;
        let mut max_lat = f64::MIN;
        let mut min_lon = f64::MAX;
        let mut max_lon = f64::MIN;

        for [lon, lat] in points {
            min_lat = min_lat.min(*lat);
            max_lat = max_lat.max(*lat);
            min_lon = min_lon.min(*lon);
            max_lon = max_lon.max(*lon);
        }

        Self {
            rings,
            min_lat,
            max_lat,
            min_lon,
            max_lon,
        }
    }

    /// Whether the point lies inside the outline: within its first ring, and
    /// not inside any of the holes that follow.
    fn contains(&self, lat: f64, lon: f64) -> bool {
        if lat < self.min_lat || lat > self.max_lat || lon < self.min_lon || lon > self.max_lon {
            return false;
        }

        let mut rings = self.rings.iter();
        let Some(border) = rings.next() else {
            return false;
        };

        ring_contains(border, lat, lon) && !rings.any(|hole| ring_contains(hole, lat, lon))
    }
}

/// Ray casting: a point is inside a ring when a ray drawn east from it crosses
/// the ring an odd number of times.
fn ring_contains(ring: &Ring, lat: f64, lon: f64) -> bool {
    let mut inside = false;

    for edge in ring.windows(2) {
        let ([lon1, lat1], [lon2, lat2]) = (edge[0], edge[1]);

        // Only edges the ray can cross, i.e. those spanning our latitude.
        if (lat1 > lat) == (lat2 > lat) {
            continue;
        }

        let crossing_lon = lon1 + (lat - lat1) / (lat2 - lat1) * (lon2 - lon1);
        if lon < crossing_lon {
            inside = !inside;
        }
    }

    inside
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn boundaries() -> CountryBoundaries {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/countries-geo");
        CountryBoundaries::load_from_dir(dir).expect("country outlines")
    }

    #[test]
    fn locates_points_inside_countries() {
        let boundaries = boundaries();
        assert!(boundaries.len() > 150, "expected the full set of outlines");

        // Cities, well inside their countries.
        assert_eq!(boundaries.country_of(52.52, 13.405), Some("germany"));
        assert_eq!(boundaries.country_of(48.8566, 2.3522), Some("france"));
        assert_eq!(boundaries.country_of(35.6762, 139.6503), Some("japan"));
        assert_eq!(boundaries.country_of(-33.8688, 151.2093), Some("australia"));

        // Summits, including ones whose closest town is across the border.
        assert_eq!(boundaries.country_of(-3.0674, 37.3556), Some("tanzania"));
        assert_eq!(
            boundaries.country_of(63.0692, -151.007),
            Some("united_states")
        );
        assert_eq!(boundaries.country_of(27.7172, 85.324), Some("nepal"));

        // The outlines are simplified, so a summit sitting on a border can
        // land on either side of it — Everest is shared with Nepal.
        assert_eq!(boundaries.country_of(27.9881, 86.925), Some("china"));
    }

    #[test]
    fn leaves_the_open_sea_unnamed() {
        assert_eq!(boundaries().country_of(30.0, -40.0), None);
    }

    #[test]
    fn holes_are_not_part_of_the_country() {
        // A single ring around the equator's origin, with a hole in it.
        let outline = Outline::new(vec![
            vec![
                [0.0, 0.0],
                [0.0, 10.0],
                [10.0, 10.0],
                [10.0, 0.0],
                [0.0, 0.0],
            ],
            vec![[4.0, 4.0], [4.0, 6.0], [6.0, 6.0], [6.0, 4.0], [4.0, 4.0]],
        ]);

        assert!(outline.contains(2.0, 2.0));
        assert!(!outline.contains(5.0, 5.0), "the hole is outside");
        assert!(!outline.contains(20.0, 20.0));
    }
}
