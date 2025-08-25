use std::time::Instant;

use config::Config;

mod commands;
mod config;
mod file_ops;
mod geonames;
mod models;

use crate::commands::cities;
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
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{}", format!("Error: {err}").red());
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
    }

    let duration = start.elapsed();
    println!("{}", format!("Command finished in {duration:.2?}").blue());

    Ok(())
}
