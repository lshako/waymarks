//! Geographic helpers: distances between coordinates, and a spatial index
//! over Geonames entries — the cities that place an activity in a country,
//! and the mountains that give a summit its name.

use crate::geonames::Geoname;
use std::collections::HashMap;

const EARTH_RADIUS_M: f64 = 6_371_008.8;
/// Side of an index cell, in degrees.
const CELL_DEGREES: f64 = 1.0;
/// Rough length of one degree of latitude, in kilometers.
const KM_PER_DEGREE_LAT: f64 = 111.32;

/// Great-circle distance between two coordinates, in meters.
pub fn haversine_meters(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (phi1, phi2) = (lat1.to_radians(), lat2.to_radians());
    let delta_phi = (lat2 - lat1).to_radians();
    let delta_lambda = (lon2 - lon1).to_radians();

    let a = (delta_phi / 2.0).sin().powi(2)
        + phi1.cos() * phi2.cos() * (delta_lambda / 2.0).sin().powi(2);

    2.0 * EARTH_RADIUS_M * a.sqrt().asin()
}

/// A grid over Geonames entries, so the nearest one to a coordinate can be
/// found without scanning the whole dump for every activity.
pub struct GeonameIndex {
    cities: Vec<Geoname>,
    cells: HashMap<(i32, i32), Vec<usize>>,
}

impl GeonameIndex {
    pub fn build(entries: Vec<Geoname>) -> Self {
        let mut cells: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (position, city) in entries.iter().enumerate() {
            cells
                .entry(cell_of(city.latitude, city.longitude))
                .or_default()
                .push(position);
        }
        Self {
            cities: entries,
            cells,
        }
    }

    /// The entry closest to `(lat, lon)` and its distance in meters, provided
    /// one lies within `max_distance_km`.
    pub fn nearest(&self, lat: f64, lon: f64, max_distance_km: f64) -> Option<(&Geoname, f64)> {
        self.nearest_matching(lat, lon, max_distance_km, |_| true)
    }

    /// The closest entry the predicate accepts — used to insist that a named
    /// peak stands at roughly the height that was actually recorded, so a
    /// wider search does not start naming summits after their neighbours.
    pub fn nearest_matching(
        &self,
        lat: f64,
        lon: f64,
        max_distance_km: f64,
        accept: impl Fn(&Geoname) -> bool,
    ) -> Option<(&Geoname, f64)> {
        let (lat_cell, lon_cell) = cell_of(lat, lon);

        // A degree of longitude shrinks towards the poles, so the search has
        // to span more longitude cells the further north or south we are.
        let lat_rings = (max_distance_km / (KM_PER_DEGREE_LAT * CELL_DEGREES)).ceil() as i32 + 1;
        let km_per_degree_lon = (KM_PER_DEGREE_LAT * lat.to_radians().cos()).max(0.1);
        let lon_rings = ((max_distance_km / (km_per_degree_lon * CELL_DEGREES)).ceil() as i32 + 1)
            .min(180 / CELL_DEGREES as i32);

        let max_distance_m = max_distance_km * 1000.0;
        let mut best: Option<(&Geoname, f64)> = None;

        for d_lat in -lat_rings..=lat_rings {
            for d_lon in -lon_rings..=lon_rings {
                let key = (lat_cell + d_lat, wrap_lon_cell(lon_cell + d_lon));
                let Some(positions) = self.cells.get(&key) else {
                    continue;
                };

                for city in positions.iter().map(|&position| &self.cities[position]) {
                    let distance = haversine_meters(lat, lon, city.latitude, city.longitude);
                    if distance > max_distance_m || !accept(city) {
                        continue;
                    }
                    if best.is_none_or(|(_, closest)| distance < closest) {
                        best = Some((city, distance));
                    }
                }
            }
        }

        best
    }
}

fn cell_of(lat: f64, lon: f64) -> (i32, i32) {
    (
        (lat / CELL_DEGREES).floor() as i32,
        (lon / CELL_DEGREES).floor() as i32,
    )
}

/// Keeps a longitude cell index inside the world, so a search near the
/// antimeridian continues on the other side of it.
fn wrap_lon_cell(cell: i32) -> i32 {
    let cells_around = 360 / CELL_DEGREES as i32;
    let half = cells_around / 2;
    (cell + half).rem_euclid(cells_around) - half
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geonames::FeatureClass;
    use chrono::NaiveDate;

    fn city(name: &str, lat: f64, lon: f64, country: &str) -> Geoname {
        Geoname {
            geonameid: 1,
            name: name.to_string(),
            asciiname: Some(name.to_string()),
            alternatenames: None,
            latitude: lat,
            longitude: lon,
            feature_class: FeatureClass::P,
            feature_code: "PPL".to_string(),
            country_code: Some(country.to_string()),
            cc2: None,
            admin1_code: None,
            admin2_code: None,
            admin3_code: None,
            admin4_code: None,
            population: None,
            elevation: None,
            dem: None,
            timezone: None,
            modification_date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        }
    }

    #[test]
    fn haversine_matches_known_distance() {
        // Berlin to Munich is roughly 504 km.
        let distance = haversine_meters(52.52, 13.405, 48.13743, 11.57549);
        assert!(
            (distance - 504_000.0).abs() < 5_000.0,
            "unexpected distance: {distance}"
        );

        assert_eq!(haversine_meters(52.52, 13.405, 52.52, 13.405), 0.0);
    }

    #[test]
    fn nearest_finds_the_closest_city() {
        let cities = vec![
            city("Berlin", 52.52, 13.405, "DE"),
            city("Munich", 48.13743, 11.57549, "DE"),
            city("Innsbruck", 47.26266, 11.39454, "AT"),
        ];
        let index = GeonameIndex::build(cities);

        // The Zugspitze summit: Innsbruck is the closest of the three.
        let (nearest, distance) = index.nearest(47.4211, 10.9863, 300.0).unwrap();
        assert_eq!(nearest.name, "Innsbruck");
        assert!(distance < 50_000.0, "unexpected distance: {distance}");

        // Its country is the one we would record for the activity.
        assert_eq!(nearest.country_code.as_deref(), Some("AT"));
    }

    #[test]
    fn nearest_searches_across_cell_borders() {
        let cities = vec![city("Berlin", 52.52, 13.405, "DE")];
        let index = GeonameIndex::build(cities);

        // Just inside the neighbouring cell to the west and south.
        let (nearest, _) = index.nearest(52.49, 12.99, 300.0).unwrap();
        assert_eq!(nearest.name, "Berlin");
    }

    #[test]
    fn nearest_gives_up_beyond_the_limit() {
        let cities = vec![city("Berlin", 52.52, 13.405, "DE")];
        let index = GeonameIndex::build(cities);

        // The middle of the Atlantic has no city within 300 km.
        assert!(index.nearest(30.0, -40.0, 300.0).is_none());
        // Neither does a point 600 km away from the only known city.
        assert!(index.nearest(47.0, 13.405, 300.0).is_none());
    }

    #[test]
    fn nearest_matching_skips_what_the_predicate_rejects() {
        // The basin lies almost on top of the recorded point, the peak
        // several hundred meters away.
        let mut low = city("Aspen Basin", 35.7965, -105.8050, "US");
        low.elevation = Some(2800);
        let mut high = city("Aspen Peak", 35.8000, -105.8100, "US");
        high.elevation = Some(4090);

        // The nearer entry stands 1290 m below the recorded peak, so only the
        // farther one can be the mountain that was climbed.
        let index = GeonameIndex::build(vec![low, high]);
        let (nearest, _) = index
            .nearest_matching(35.7962, -105.8047, 5.0, |feature| {
                feature
                    .elevation
                    .is_none_or(|height| (f64::from(height) - 4098.0).abs() <= 150.0)
            })
            .unwrap();

        assert_eq!(nearest.name, "Aspen Peak");
        // Without the guard the closer, lower one wins.
        assert_eq!(
            index.nearest(35.7962, -105.8047, 5.0).unwrap().0.name,
            "Aspen Basin"
        );
    }

    #[test]
    fn nearest_wraps_around_the_antimeridian() {
        let cities = vec![city("Wellington", -41.28664, 174.77557, "NZ")];
        let index = GeonameIndex::build(cities);

        // Approaching from the eastern side of the antimeridian.
        let (nearest, _) = index.nearest(-41.3, -179.5, 600.0).unwrap();
        assert_eq!(nearest.name, "Wellington");
    }
}
