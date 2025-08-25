# Waymarks

Waymarks is a Rust-based CLI and web application I built to track my travels. It helps manage and visualize geographical data like countries, cities, and mountain summits, letting me:

- Track visited cities ✅
- Track visited summits (TODO)
- Display countries, cities, and summits on an interactive Leaflet map
- Download and process Geonames datasets automatically
- Integrate with Strava to track visited cities and summits from your activities (TODO)

## Features

- **CLI Operations**
    - Add single or multiple cities
    - Update country list automatically
    - Automatic fetching of Geonames data if missing
- **Web Map**
    - Interactive map with layers for countries, cities, and summits
    - Distinct marker colors for cities and summits
    - Popups showing city names, country, summit names, elevation, and date
- **Data Storage**
    - Persistent JSON storage for cities and countries
    - Supports incremental additions

## TODO
- [ ] Integrate **Strava API** to track activities:
    - [ ] Pull summit data from Strava
    - [ ] Track visited cities from Strava activities
- [ ] Set up a CI/CD pipeline using GitHub Actions
- [ ] Schedule automatic data updates via GitHub Actions cron jobs

## License
MIT License

