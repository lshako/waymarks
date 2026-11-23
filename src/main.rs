use std::time::Instant;

use config::Config;

mod commands;
mod config;
mod file_ops;
mod garmin;
mod geo;
mod geonames;
mod models;

use crate::commands::cities;
use crate::commands::garmin::{ImportOptions, import};
use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::*;

#[derive(Parser)]
#[command(name = "Waymarks CLI")]
struct Cli {
    #[arg(short, long, default_value = "config.toml")]
    config: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(alias = "ac")]
    AddCities { country: String, names: Vec<String> },

    /// Import the summits, cities and countries of your Garmin activities.
    ///
    /// Downloads whatever is new first — the whole history on the first run —
    /// then adds what the map is missing.
    #[command(alias = "ig")]
    ImportGarmin {
        /// Import what is already downloaded, without syncing first.
        #[arg(long)]
        no_sync: bool,

        /// Sync the whole history again, not only what is new.
        #[arg(long)]
        backfill: bool,

        /// Report what would change without writing any file.
        #[arg(long)]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{}", format!("Error: {err}").red());
        // The context alone rarely says what went wrong, so show what it wraps.
        for cause in err.chain().skip(1) {
            eprintln!("{}", format!("  caused by: {cause}").red());
        }
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    let cfg = Config::from_file(cli.config)?;

    let start = Instant::now();

    match cli.command {
        Commands::AddCities { country, names } => {
            cities::add_cities(&cfg, &country, &names).await?;
        }
        Commands::ImportGarmin {
            no_sync,
            backfill,
            dry_run,
        } => {
            import(
                &cfg,
                ImportOptions {
                    sync: !no_sync,
                    backfill,
                    dry_run,
                },
            )
            .await?;
        }
    }

    let duration = start.elapsed();
    println!("{}", format!("Command finished in {duration:.2?}").blue());

    Ok(())
}
