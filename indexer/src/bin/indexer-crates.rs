use std::{env::temp_dir, os, path::PathBuf};

use anyhow::{Context, Result};
use crates_io_api::{AsyncClient, Crate, CratesResponse, ListOptions, Sort};

use tokio::{
    fs::{self, OpenOptions},
    io::copy,
    process::Command,
};

async fn index_krate(krate: &Crate) -> Result<()> {
    let temp = temp_dir();
    let path = temp.join(format!("{}.tar.gz", krate.name));
    let url = format!(
        "https://static.crates.io/crates/{name}/{name}-{version}.crate",
        name = krate.name,
        version = krate.max_version,
    );

    let resp = reqwest::get(url).await?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .open(path)
        .await
        .context("Could not create the temp tar.gz file")?;

    copy(&mut resp.bytes().await?.as_ref(), &mut file)
        .await
        .context("tokio::io::copy failed")?;

    Command::new("tar")
        .args(&["-xf", &format!("{}.tar.gz", krate.name)])
        .current_dir(&temp)
        .status()
        .await
        .context("Failed to extract tar.gz file")?;

    let unpacked = temp.join(format!("{}-{}", krate.name, krate.max_version));
    let cargo = Command::new("cargo")
        .args(&["+nightly", "rustdoc"])
        .env("RUSTDOCFLAGS", "--output-format=json -Z unstable-options")
        .current_dir(&unpacked)
        .status()
        .await
        .context("Failed to run cargo rustdoc")?;
    if !cargo.success() {
        return Err(anyhow::anyhow!(
            "cargo rustdoc failed for crate {}",
            krate.name
        ));
    }
    // check the `target/doc` contents
    let doc_dir = unpacked.join("target/doc");
    if !doc_dir.exists() {
        return Err(anyhow::anyhow!(
            "doc directory does not exist for crate {}",
            krate.name
        ));
    }
    let mut doc_dir_reader = fs::read_dir(&doc_dir).await?;
    while let Ok(Some(entry)) = doc_dir_reader.next_entry().await {
        println!("Found doc file: {:?}", entry.file_name());
        let new_name = format!("{}.json", krate.name);
        fs::rename(entry.path(), doc_dir.join(&new_name)).await?;
        println!("Renamed to: {:?}", new_name);
    }
    println!("All doc files processed for crate {}", krate.name);

    println!(
        "Moving JSON file for crate {} to index directory",
        krate.name
    );
    let mv = Command::new("mv")
        .args(&[
            unpacked.join(format!("target/doc/{}.json", krate.name)),
            PathBuf::from("crate"),
        ])
        .status()
        .await
        .context("Failed to move JSON file")?;
    println!(
        "Moved JSON file for crate {} to index directory",
        krate.name
    );
    if !mv.success() {
        return Err(anyhow::anyhow!(
            "Failed to move JSON file for crate {}",
            krate.name
        ));
    }

    Ok(())
}

async fn popular_crates() -> Result<()> {
    let client = AsyncClient::new(
        "roogle (git@hkmatsumoto.com)",
        std::time::Duration::from_millis(1000),
    )?;

    let CratesResponse { crates: krates, .. } = client
        .crates(ListOptions {
            sort: Sort::Downloads,
            per_page: 100,
            page: 3,
            query: None,
        })
        .await?;
    let mut json = vec![];
    for krate in krates {
        if index_krate(&krate).await.is_ok() {
            json.push(krate.name);
        } else {
            eprintln!("failed to index crate: {}", krate.name);
        }
    }

    let json = serde_json::to_string(&json)?;
    std::fs::write("../set/crates.json", &json)?;

    Ok(())
}

async fn stale_crates() -> Result<()> {
    let client = AsyncClient::new(
        "roogle (git@hkmatsumoto.com)",
        std::time::Duration::from_millis(1000),
    )?;
    let stales = fs::read_to_string("stale-crates.txt")
        .await?
        .lines()
        .map(|s| s.to_owned())
        .collect::<Vec<_>>();

    let mut json = vec![];
    for krate_name in stales {
        let krate = client
            .get_crate(&krate_name)
            .await
            .context(format!("failed to get crate info: {}", &krate_name));
        if krate.is_err() {
            eprintln!("skipping crate: {}", &krate_name);
            continue;
        }
        let krate = krate.unwrap().crate_data;

        if index_krate(&krate).await.is_ok() {
            json.push(krate.name);
        }
    }
    let json = serde_json::to_string(&json)?;
    std::fs::write("../set/crates.json", &json)?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    if std::env::args().any(|arg| arg == "--stale") {
        stale_crates().await
    } else {
        popular_crates().await
    }
}
