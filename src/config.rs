use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub geonames: GeoNames,
    pub docs: Docs,
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
}
