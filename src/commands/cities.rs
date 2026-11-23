use crate::config::Config;
use crate::geonames::{self, CountryMaps, normalize_country_name};
use crate::models::{cities::Coordinates, countries::Countries};
use anyhow::Result;
use colored::Colorize;
use std::collections::HashMap;

pub(crate) async fn add_cities(config: &Config, country: &str, names: &[String]) -> Result<()> {
    let (country_iso, country_name) = update_country(config, country).await?;

    let country_file = config.cities_path(&country_name);

    let mut cities = crate::models::cities::Cities::load_from_file(&country_file)
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
        cities.save_to_file(&country_file)?;
    }

    Ok(())
}

async fn get_cities(
    config: &Config,
    names: &[String],
    country_iso: &str,
) -> Result<HashMap<String, Option<geonames::Geoname>>> {
    let cities = geonames::load_cities(config).await?;

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

    let countries_path = config.countries_path();
    let mut counties = Countries::load_from_file(&countries_path)?;

    if counties.add(&country_name) {
        counties.save_to_file(&countries_path)?;
        println!("Added country: {country_name}");
    } else {
        println!("Country '{country_name}' already exists");
    }
    Ok((country_iso, country_name))
}

async fn get_country_info(config: &Config, country: &str) -> Result<(String, String)> {
    let countries = geonames::load_country_info(config).await?;
    let country_maps = CountryMaps::from_country_info(&countries);

    if let Some((iso, name)) = country_maps.resolve_country(country) {
        Ok((iso, normalize_country_name(&name)))
    } else {
        Err(anyhow::anyhow!("Invalid country name or ISO: {}", country))
    }
}
