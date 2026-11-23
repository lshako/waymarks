use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub geonames: GeoNames,
    pub docs: Docs,
    #[serde(default)]
    pub garmin: Garmin,
}

#[derive(Debug, Deserialize)]
pub struct GeoNames {
    pub base_url: String,
    pub country_info_file: String,
    pub cities_file: String,
    pub download_dir: PathBuf,
}

#[derive(Debug, Deserialize)]
pub struct Docs {
    pub dir: PathBuf,
    pub countries_file: String,
    pub cities_folder: String,
    #[serde(default = "default_summits_file")]
    pub summits_file: String,
    /// Folder of the country outlines the map draws, one file per country.
    #[serde(default = "default_countries_geo_folder")]
    pub countries_geo_folder: String,
}

/// Settings of the Garmin import: where the synced store lives and which
/// activities can have reached a summit.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Garmin {
    /// The `garmin` executable that fills the store.
    pub command: String,
    /// Where `garmin sync` keeps its Parquet files. Defaults to the same
    /// place the tool does, and `~` is expanded.
    pub storage_dir: PathBuf,
    /// The activity types that can reach a summit. Empty accepts every type.
    pub activity_types: Vec<String>,
    /// An activity only counts as a summit when its highest point is at
    /// least this many meters above sea level.
    pub min_elevation: f64,
    /// How close a city has to be to where an activity started to count as
    /// visited.
    pub city_radius_km: f64,
    /// How far the nearest known city may be before the country of an
    /// activity is treated as unknown.
    pub max_city_distance_km: f64,
    /// How close a named peak has to be to the highest recorded point for the
    /// summit to take its name.
    pub summit_name_radius_m: f64,
    /// The Geonames feature codes that count as a named summit.
    pub summit_feature_codes: Vec<String>,
    /// How far a named peak's surveyed height may be from the recorded one
    /// before it is taken to be a different mountain. Wide, because a watch
    /// reading barometric altitude drifts.
    pub summit_name_elevation_tolerance_m: f64,
}

impl Default for Garmin {
    fn default() -> Self {
        Self {
            command: "garmin".to_string(),
            storage_dir: default_storage_dir(),
            activity_types: vec![
                "hiking".to_string(),
                "mountaineering".to_string(),
                "rock_climbing".to_string(),
                "trail_running".to_string(),
                "backcountry_skiing".to_string(),
                "resort_skiing".to_string(),
                "ski_touring".to_string(),
            ],
            min_elevation: 1000.0,
            city_radius_km: 25.0,
            max_city_distance_km: 300.0,
            summit_name_radius_m: 1000.0,
            summit_name_elevation_tolerance_m: 400.0,
            summit_feature_codes: [
                "PK", "PKS", "MT", "MTS", "VLC", "HLL", "HLLS", "RDGE", "SDL", "PASS",
            ]
            .iter()
            .map(|code| code.to_string())
            .collect(),
        }
    }
}

/// Where `garmin sync` stores its files when nothing else is configured.
fn default_storage_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/share"))
        .join("garmin")
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn default_summits_file() -> String {
    "summits.json".to_string()
}

fn default_countries_geo_folder() -> String {
    "countries-geo".to_string()
}

impl Config {
    pub fn from_file(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let cfg: Config = toml::from_str(&content)?;
        Ok(cfg)
    }

    pub fn country_info_url(&self) -> String {
        format!(
            "{}{}",
            self.geonames.base_url, self.geonames.country_info_file
        )
    }

    pub fn cities_url(&self) -> String {
        format!("{}{}", self.geonames.base_url, self.geonames.cities_file)
    }

    pub fn countries_path(&self) -> PathBuf {
        self.docs.dir.join(&self.docs.countries_file)
    }

    pub fn summits_path(&self) -> PathBuf {
        self.docs.dir.join(&self.docs.summits_file)
    }

    pub fn countries_geo_dir(&self) -> PathBuf {
        self.docs.dir.join(&self.docs.countries_geo_folder)
    }

    /// The Garmin store, with a leading `~` expanded to the home directory.
    pub fn garmin_store(&self) -> PathBuf {
        match self.garmin.storage_dir.strip_prefix("~") {
            Ok(rest) => home().join(rest),
            Err(_) => self.garmin.storage_dir.clone(),
        }
    }

    /// The city file of a country, named after the normalized country name.
    pub fn cities_path(&self, country_name: &str) -> PathBuf {
        self.docs
            .dir
            .join(&self.docs.cities_folder)
            .join(format!("{country_name}.json"))
    }
}
