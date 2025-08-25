use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs};
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Coordinates {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Cities {
    pub cities: BTreeMap<String, Coordinates>,
}

impl Cities {
    pub fn new() -> Self {
        Self {
            cities: BTreeMap::new(),
        }
    }

    pub fn add(&mut self, name: String, coordinates: Coordinates) -> bool {
        self.cities.insert(name, coordinates).is_none()
    }

    pub fn save_to_file(&self, path: &str) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.cities)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load_from_file(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let cities: BTreeMap<String, Coordinates> = serde_json::from_str(&content)?;
        Ok(Self { cities })
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_add_and_save_load() -> Result<()> {
        let mut cities = Cities::new();
        let berlin = Coordinates {
            lat: 52.5200,
            lon: 13.4050,
        };
        let munich = Coordinates {
            lat: 48.13743,
            lon: 11.57549,
        };

        assert!(cities.add("Berlin".to_string(), berlin.clone()));
        assert!(cities.add("Munich".to_string(), munich.clone()));
        assert!(!cities.add("Berlin".to_string(), berlin.clone()));

        let mut tmp_path = std::env::temp_dir();
        tmp_path.push("cities_test.json");
        let path_str = tmp_path.to_str().unwrap();

        cities.save_to_file(path_str)?;

        let loaded = Cities::load_from_file(path_str)?;
        assert_eq!(loaded.cities.get("Berlin"), Some(&berlin));
        assert_eq!(loaded.cities.get("Munich"), Some(&munich));

        fs::remove_file(path_str)?;
        Ok(())
    }
}
