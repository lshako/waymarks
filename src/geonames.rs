use anyhow::Result;
use chrono::NaiveDate;
use csv::ReaderBuilder;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Geoname {
    pub geonameid: i64,
    pub name: String,
    pub asciiname: Option<String>,
    pub alternatenames: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    pub feature_class: FeatureClass,
    pub feature_code: String,
    pub country_code: Option<String>,
    pub cc2: Option<String>,
    pub admin1_code: Option<String>,
    pub admin2_code: Option<String>,
    pub admin3_code: Option<String>,
    pub admin4_code: Option<String>,
    pub population: Option<f64>,
    pub elevation: Option<i32>,
    pub dem: Option<f64>,
    pub timezone: Option<String>,

    #[serde(
        serialize_with = "date_format::serialize",
        deserialize_with = "date_format::deserialize"
    )]
    pub modification_date: NaiveDate,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct CountryInfo {
    pub iso: String,
    pub iso3: String,
    pub iso_numeric: String,
    pub fips: Option<String>,
    pub country: String,
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum FeatureClass {
    A, // country, state, region,...
    H, // stream, lake, ...
    L, // parks, area, ...
    P, // city, village,...
    R, // road, railroad
    S, // spot, building, farm
    T, // mountain, hill, rock,...
    U, // undersea
    V, // forest, heath,...
}

pub fn read_tsv<T, P>(path: P) -> Result<Vec<T>>
where
    T: DeserializeOwned,
    P: AsRef<Path>,
{
    let mut rdr = ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(false)
        .comment(Some(b'#'))
        .from_path(path)
        .map_err(|e| anyhow::anyhow!("Failed to read TSV file: {}", e))?;

    let mut records = Vec::new();
    for result in rdr.deserialize() {
        let record: T =
            result.map_err(|e| anyhow::anyhow!("Failed to deserialize record: {}", e))?;
        records.push(record);
    }
    Ok(records)
}

impl fmt::Display for FeatureClass {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let symbol = match self {
            FeatureClass::A => "A",
            FeatureClass::H => "H",
            FeatureClass::L => "L",
            FeatureClass::P => "P",
            FeatureClass::R => "R",
            FeatureClass::S => "S",
            FeatureClass::T => "T",
            FeatureClass::U => "U",
            FeatureClass::V => "V",
        };
        write!(f, "{symbol}")
    }
}

mod date_format {
    use chrono::NaiveDate;
    use serde::{self, Deserialize, Deserializer, Serializer};

    const DATE_FORMAT: &str = "%Y-%m-%d";

    pub fn serialize<S>(date: &NaiveDate, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = date.format(DATE_FORMAT).to_string();
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        NaiveDate::parse_from_str(&s, DATE_FORMAT).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use std::{env, path::PathBuf};

    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn test_geoname_date_serialization() {
        let geoname = Geoname {
            geonameid: 123,
            name: "Testville".to_string(),
            asciiname: None,
            alternatenames: None,
            latitude: 52.52,
            longitude: 13.405,
            feature_class: FeatureClass::P,
            feature_code: "PPL".to_string(),
            country_code: Some("DE".to_string()),
            cc2: None,
            admin1_code: Some("16".to_string()),
            admin2_code: None,
            admin3_code: None,
            admin4_code: None,
            population: Some(3_600_000.0),
            elevation: Some(34),
            dem: Some(34.5),
            timezone: Some("Europe/Berlin".to_string()),
            modification_date: NaiveDate::from_ymd_opt(2025, 8, 18).unwrap(),
        };

        let json = serde_json::to_string(&geoname).unwrap();
        assert!(json.contains(r#""modification_date":"2025-08-18""#));

        let deserialized: Geoname = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.modification_date,
            NaiveDate::from_ymd_opt(2025, 8, 18).unwrap()
        );
    }

    #[test]
    fn test_read_tsv_country_info() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/countryInfo_sample.txt");

        let countries: Vec<CountryInfo> = read_tsv(&path).expect("Failed to read country info TSV");
        assert!(!countries.is_empty(), "Country info should not be empty");

        assert_eq!(countries[0].iso, "AD");
        assert_eq!(countries[0].iso3, "AND");
        assert_eq!(countries[0].iso_numeric, "020");
        assert_eq!(countries[0].fips, Some("AN".to_string()));
        assert_eq!(countries[0].country, "Andorra");

        assert_eq!(countries[1].iso, "AE");
        assert_eq!(countries[1].iso3, "ARE");
        assert_eq!(countries[1].iso_numeric, "784");
        assert_eq!(countries[1].fips, Some("AE".to_string()));
        assert_eq!(countries[1].country, "United Arab Emirates");
    }

    #[test]
    fn test_read_tsv_geoname() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/cities_sample.txt");

        let geonames: Vec<Geoname> = read_tsv(&path).expect("Failed to read geoname TSV");
        assert!(!geonames.is_empty(), "Geonames should not be empty");
        assert_eq!(geonames[0].geonameid, 3038832);
        assert_eq!(geonames[0].name, "Vila");
        assert_eq!(geonames[0].asciiname, Some("Vila".to_string()));
        assert_eq!(
            geonames[0].alternatenames,
            Some("Casas Vila,Vila".to_string())
        );
        assert_eq!(geonames[0].latitude, 42.53176);
        assert_eq!(geonames[0].longitude, 1.56654);
        assert_eq!(geonames[0].feature_class, FeatureClass::P);
        assert_eq!(geonames[0].feature_code, "PPL");
        assert_eq!(geonames[0].country_code, Some("AD".to_string()));
        assert_eq!(geonames[0].cc2, None, "cc2 should be None");
        assert_eq!(geonames[0].admin1_code, Some("03".to_string()));
        assert_eq!(geonames[0].admin2_code, None, "admin2_code should be None");
        assert_eq!(geonames[0].admin3_code, None, "admin3_code should be None");
        assert_eq!(geonames[0].admin4_code, None, "admin4_code should be None");
        assert_eq!(geonames[0].population, Some(1418.0));
        assert_eq!(geonames[0].elevation, None);
        assert_eq!(geonames[0].dem, Some(1318f64));
        assert_eq!(geonames[0].timezone, Some("Europe/Andorra".to_string()));
        assert_eq!(
            geonames[0].modification_date,
            NaiveDate::from_ymd_opt(2024, 11, 4).unwrap()
        );
        assert_eq!(geonames[1].geonameid, 3038999);
        assert_eq!(geonames[1].name, "Soldeu");
        assert_eq!(geonames[1].asciiname, Some("Soldeu".to_string()));
        assert_eq!(
            geonames[1].alternatenames,
            Some("Sol'deu,Soldeu,surudeu,swldw,Сольдеу,סולדאו,سولدو,スルデウ".to_string())
        );
        assert_eq!(geonames[1].latitude, 42.57688);
        assert_eq!(geonames[1].longitude, 1.66769);
        assert_eq!(geonames[1].feature_class, FeatureClass::P);
        assert_eq!(geonames[1].feature_code, "PPL");
        assert_eq!(geonames[1].country_code, Some("AD".to_string()));
        assert_eq!(geonames[1].cc2, None, "cc2 should be None");
        assert_eq!(geonames[1].admin1_code, Some("02".to_string()));
        assert_eq!(geonames[1].admin2_code, None, "admin2_code should be None");
        assert_eq!(geonames[1].admin3_code, None, "admin3_code should be None");
        assert_eq!(geonames[1].admin4_code, None, "admin4_code should be None");
        assert_eq!(geonames[1].population, Some(602.0));
        assert_eq!(geonames[1].elevation, None);
        assert_eq!(geonames[1].dem, Some(1832f64));
        assert_eq!(geonames[1].timezone, Some("Europe/Andorra".to_string()));
        assert_eq!(
            geonames[1].modification_date,
            NaiveDate::from_ymd_opt(2017, 11, 6).unwrap()
        );
    }
}
