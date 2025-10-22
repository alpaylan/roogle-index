use std::process::Command;

use anyhow::{Context, Result};
use glob::glob;

fn main() -> Result<()> {
    let rust_path = std::env::var("ROOGLE_RUST_PATH")
        .context("environment variable `ROOGLE_RUST_PATH` is not set")?;
    let crates_path = std::env::var("ROOGLE_CRATES_PATH").unwrap_or("./crate".to_string());
    let set_path = std::env::var("ROOGLE_SET_PATH").unwrap_or("./set".to_string());
    std::env::set_current_dir(&rust_path)
        .with_context(|| format!("failed to change directory to `{}`", rust_path))?;

    Command::new("./x.py")
        .env("RUSTDOCFLAGS", "-Zunstable-options --output-format json")
        .args(&["doc", "compiler", "--json"])
        .status()
        .context("failed to index `set:rustc`")?;

    let mut krates = vec![];
    for json in glob("./build/**/compiler-doc/*.json").context("failed to list indexes")? {
        let json = json?;
        let krate = json
            .file_stem()
            .with_context(|| format!("failed to get name of `{}`", json.display()))?;
        let krate = krate
            .to_str()
            .with_context(|| format!("failed to get `&str` from `{:?}`", krate))?;
        let from = format!("build/x86_64-unknown-linux-gnu/compiler-doc/{}.json", krate);

        krates.push(krate.to_owned());

        Command::new("cp")
            .args(&[&from, &crates_path])
            .status()
            .with_context(|| format!("failed to store `{}.json` to index", krate))?;
    }

    let json =
        serde_json::to_string(&krates).context("serializing crates of `set:rustc` failed")?;
    std::fs::write(&format!("{}/rustc.json", set_path), &json)
        .context("failed to write `set:rustc` metadata")?;

    Ok(())
}
