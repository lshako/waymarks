use anyhow::Result;
use serde::Deserialize;
use std::{collections::BTreeSet, fs};

#[derive(Debug, Deserialize)]
pub struct Countries {
    countries: BTreeSet<String>,
}

impl Countries {
    pub fn load_from_file(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let countries: BTreeSet<String> = serde_json::from_str(&content)?;
        Ok(Self { countries })
    }
    pub fn save_to_file(&self, path: &str) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.countries)?;
        fs::write(path, json)?;
        Ok(())
    }
    pub fn add(&mut self, country: &str) -> bool {
        self.countries.insert(country.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs};

    #[test]
    fn test_countries() -> Result<()> {
        let temp_dir = env::temp_dir();

        let temp_file = temp_dir.join("test_countries.json");
        let temp_file_str = temp_file.to_str().unwrap();

        if temp_file.exists() {
            fs::remove_file(&temp_file)?;
        }

        let mut countries = Countries {
            countries: Default::default(),
        };
        assert!(countries.add("Germany"));
        assert!(countries.add("France"));
        assert!(countries.add("Japan"));

        countries.save_to_file(temp_file_str)?;

        let loaded = Countries::load_from_file(temp_file_str)?;
        assert!(loaded.countries.contains("Germany"));
        assert!(loaded.countries.contains("France"));
        assert!(loaded.countries.contains("Japan"));

        fs::remove_file(temp_file)?;

        Ok(())
    }
}
