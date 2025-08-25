use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use std::fs;
use std::fs::File;
use std::io::{self};
use std::path::Path;
use std::time::Duration;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::task;
use zip::ZipArchive;

async fn download_file<P>(url: &str, output_path: P) -> Result<()>
where
    P: AsRef<Path>,
{
    let path = output_path.as_ref();

    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .context("Failed to build HTTP client")?;

    let mut response = client
        .get(url)
        .send()
        .await
        .context("Failed to send request")?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to download: {}", response.status());
    }

    let total_size = response
        .content_length()
        .context("Missing content-length header")?;

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );

    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .await
        .with_context(|| format!("Failed to create file {}", path.display()))?;

    let mut downloaded: u64 = 0;
    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
    }

    pb.finish_with_message("Download complete ✅");
    Ok(())
}

pub(crate) async fn ensure_file<P>(url: &str, output_path: P) -> Result<()>
where
    P: AsRef<Path>,
{
    let path = output_path.as_ref();

    if path.exists() {
        println!("File {} already exists, skipping download.", path.display());
        return Ok(());
    }

    println!("Downloading {url} → {}...", path.display());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    download_file(url, output_path).await?;
    Ok(())
}

pub(crate) async fn unzip_file(zip_path: &str, output_dir: &str) -> anyhow::Result<()> {
    let zip_path = zip_path.to_string();
    let output_dir = output_dir.to_string();

    task::spawn_blocking(move || -> Result<()> {
        let file = File::open(&zip_path)?;
        let mut archive = ZipArchive::new(file)?;

        for i in 0..archive.len() {
            let mut zipped_file = archive.by_index(i)?;
            let outpath = std::path::Path::new(&output_dir).join(zipped_file.name());
            if outpath.exists() {
                println!("Skipping {}, already exists", outpath.display());
                continue;
            }

            if zipped_file.is_dir() {
                std::fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    std::fs::create_dir_all(p)?;
                }
                let mut outfile = File::create(&outpath)?;
                io::copy(&mut zipped_file, &mut outfile)?;
                println!("Extracted {}", outpath.display());
            }
        }

        Ok(())
    })
    .await??;

    Ok(())
}
