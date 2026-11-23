# Waymarks

Waymarks is a Rust-based CLI and web application I built to track my travels. It helps manage and visualize geographical data like countries, cities, and mountain summits, letting me:

- Track visited cities ✅
- Track visited summits ✅
- Display countries, cities, and summits on an interactive Leaflet map
- Download and process Geonames datasets automatically
- Import summits, cities and countries straight from my Garmin activities ✅

## Features

- **CLI Operations**
    - Add single or multiple cities
    - Update country list automatically
    - Automatic fetching of Geonames data if missing
    - Import summits, cities and countries from Garmin
- **Web Map**
    - Interactive map with layers for countries, cities, and summits
    - Distinct marker colors for cities and summits
    - Popups showing city names, country, summit names, elevation, and date
- **Data Storage**
    - Persistent JSON storage for cities, countries, and summits
    - Supports incremental additions

## Usage

### Adding cities by hand

```sh
cargo run -- add-cities germany Berlin Munich
```

### Importing from Garmin

Downloading is done by [garmin-cli](https://github.com/vicentereig/garmin-cli),
which is installed and authorised once:

```sh
cargo install garmin-cli
garmin auth login          # browser SSO, and MFA if the account has it
```

Then a single command downloads whatever is new and puts it on the map:

```sh
waymarks import-garmin
```

The first run reaches back through the whole history, because the local store
is empty. Every run after that only downloads the activities recorded since —
`garmin sync` keeps its own record of what it has fetched.

```sh
waymarks import-garmin --dry-run    # report what would change, write nothing
waymarks import-garmin --no-sync    # import what is already downloaded
waymarks import-garmin --backfill   # fetch the whole history again
```

**Summits** come from the recorded track: the highest point of any activity of
a mountain type (`hiking`, `mountaineering`, `ski_touring`, … — configurable)
that reaches `min_elevation`. Two summits are the same one when they stand
within 150 m of each other, so re-importing changes nothing. Names are *not*
used to tell summits apart: Garmin names an activity after the valley it starts
in, so several peaks share one name — rename an activity in Garmin Connect if
you want a better label on the map.

**Cities** are the closest known place to where an activity started, within
`city_radius_km`, taken from the Geonames dump and filed under their own
country.

**Countries** come from the start point, matched against the outlines in
`docs/countries-geo`. Those are simplified, so a start on a coast can fall
outside all of them; the closest city then decides.

Nothing is ever removed or overwritten — every import only adds what is
missing, so running it twice is harmless.

### Configuration

`config.toml` holds the Geonames sources, where the map data lives, and the
import settings:

```toml
[garmin]
command = "garmin"
storage_dir = "~/.local/share/garmin"
activity_types = ["hiking", "mountaineering", "rock_climbing", "trail_running",
                  "backcountry_skiing", "resort_skiing", "ski_touring"]
min_elevation = 1000.0
city_radius_km = 25.0
max_city_distance_km = 300.0
```

## Why not Strava

Strava moved its API to the subscriber-only Standard Tier in 2026: an
application whose owner has no subscription is marked `Inactive`, and every
endpoint — including `/athlete` — answers `403`. Device and partner
integrations such as Garmin are exempt, but that tier is not open to
individuals. Garmin is the source instead.

## TODO
- [ ] Automate the import on a schedule (systemd timer)
- [ ] Set up a CI/CD pipeline using GitHub Actions

## License
MIT License
