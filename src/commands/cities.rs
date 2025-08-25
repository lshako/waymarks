use crate::config::Config;
use crate::file_ops;
use crate::geonames;
use crate::models::{cities::Coordinates, countries::Countries};
use anyhow::Result;
use colored::Colorize;
use std::collections::HashMap;

struct CountryMaps {
    name_to_iso: HashMap<String, String>,
    iso_to_name: HashMap<String, String>,
}

impl CountryMaps {
    fn new() -> Self {
        Self {
            name_to_iso: HashMap::new(),
            iso_to_name: HashMap::new(),
        }
    }

    fn add_country(&mut self, name: String, iso: String) {
        let name = name.to_lowercase();
        let iso = iso.to_lowercase();
        self.name_to_iso.insert(name.clone(), iso.clone());
        self.iso_to_name.insert(iso, name);
    }

    fn get_iso(&self, name: &str) -> Option<&String> {
        self.name_to_iso.get(name)
    }

    fn get_name(&self, iso: &str) -> Option<&String> {
        self.iso_to_name.get(iso)
    }

    fn resolve_country(&self, name_or_iso: &str) -> Option<(String, String)> {
        let name_or_iso = name_or_iso.to_lowercase();
        if let Some(iso) = self.get_iso(&name_or_iso) {
            Some((iso.clone(), name_or_iso.to_string()))
        } else {
            self.get_name(&name_or_iso)
                .map(|name| (name_or_iso.to_string(), name.clone()))
        }
    }
}

pub(crate) async fn add_cities(config: &Config, country: &str, names: &[String]) -> Result<()> {
    let (country_iso, country_name) = update_country(config, country).await?;

    let country_file = config
        .docs
        .dir
        .join(&config.docs.cities_folder)
        .join(format!("{country_name}.json"));

    let mut cities = crate::models::cities::Cities::load_from_file(country_file.to_str().unwrap())
        .unwrap_or_else(|_| crate::models::cities::Cities::new());

    let mut is_changed = false;

    let get_cities = get_cities(config, names, &country_iso).await?;
    for (name, city) in get_cities {
        if let Some(city) = city {
            let coordinates = Coordinates {
                lat: city.latitude,
                lon: city.longitude,
            };
            if cities.add(city.name.clone(), coordinates) {
                is_changed = true;

                println!(
                    "{}",
                    format!(
                        "Added city: {} ({}, {})",
                        city.name, city.latitude, city.longitude
                    )
                    .green()
                );
            } else {
                println!(
                    "{}",
                    format!(
                        "City '{}' already exists in country '{country_name}'",
                        city.name
                    )
                    .yellow()
                );
            }
        } else {
            println!(
                "{}",
                format!("City '{name}' not found in country '{country_name}'").red()
            );
        }
    }

    if is_changed {
        cities.save_to_file(country_file.to_str().unwrap())?;
    }

    Ok(())
}

async fn get_cities(
    config: &Config,
    names: &[String],
    country_iso: &str,
) -> Result<HashMap<String, Option<geonames::Geoname>>> {
    let url_str = config.cities_url();
    let filename = url_str.rsplit('/').next().unwrap_or("cities.zip");
    let zip_file = config.geonames.download_dir.join(filename);

    file_ops::ensure_file(&url_str, &zip_file).await?;
    file_ops::unzip_file(
        zip_file.to_str().unwrap(),
        config.geonames.download_dir.to_str().unwrap(),
    )
    .await?;

    let cities_file = zip_file
        .with_extension("")
        .with_extension("txt")
        .to_str()
        .unwrap()
        .to_string();

    let cities = geonames::read_tsv::<geonames::Geoname, _>(&cities_file)?;

    // normalize requested names only once
    let names_lower: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    let mut res: HashMap<String, Option<geonames::Geoname>> =
        names_lower.iter().map(|n| (n.clone(), None)).collect();

    let mut found = 0;

    for city in cities.into_iter().filter(|c| {
        c.country_code
            .as_deref()
            .map_or_else(|| false, |code| code.eq_ignore_ascii_case(country_iso))
    }) {
        let keys = std::iter::once(city.name.to_lowercase())
            .chain(city.asciiname.clone().map(|s| s.to_lowercase()));

        for key in keys {
            if let Some(entry) = res.get_mut(&key) {
                *entry = Some(city);
                found += 1;
                break;
            }
        }

        if found == res.len() {
            break;
        }
    }

    Ok(res)
}

async fn update_country(config: &Config, country: &str) -> Result<(String, String)> {
    let (country_iso, country_name) = get_country_info(config, country).await?;

    let mut counties = Countries::load_from_file(
        config
            .docs
            .dir
            .join(&config.docs.countries_file)
            .to_str()
            .unwrap(),
    )?;

    if counties.add(&country_name) {
        counties.save_to_file(
            config
                .docs
                .dir
                .join(&config.docs.countries_file)
                .to_str()
                .unwrap(),
        )?;
        println!("Added country: {country_name}");
    } else {
        println!("Country '{country_name}' already exists");
    }
    Ok((country_iso, country_name))
}

async fn get_country_info(config: &Config, country: &str) -> Result<(String, String)> {
    let url_str = config.country_info_url();
    let filename = url_str.rsplit('/').next().unwrap_or("countryInfo.txt");
    let output_path = config.geonames.download_dir.join(filename);
    file_ops::ensure_file(&url_str, &output_path).await?;

    let countries = geonames::read_tsv::<geonames::CountryInfo, _>(&output_path)?;
    let mut country_maps = CountryMaps::new();

    for country in countries {
        country_maps.add_country(country.country.clone(), country.iso.clone());
    }

    if let Some((iso, name)) = country_maps.resolve_country(country) {
        Ok((iso, name.replace(' ', "_")))
    } else {
        Err(anyhow::anyhow!("Invalid country name or ISO: {}", country))
    }
}
