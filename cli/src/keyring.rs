//! `super keyring` — list license verifying key ids embedded in this binary.

use colored::Colorize;
use common::embedded_verifying_keys;
use serde::Serialize;

#[derive(Serialize)]
struct KeyringReport {
    version: &'static str,
    keys: Vec<KeyringEntry>,
}

#[derive(Serialize)]
struct KeyringEntry {
    kid: String,
}

pub fn run(json: bool) -> anyhow::Result<()> {
    let keys = embedded_verifying_keys();
    if json {
        let report = KeyringReport {
            version: env!("CARGO_PKG_VERSION"),
            keys: keys
                .into_iter()
                .map(|k| KeyringEntry { kid: k.kid })
                .collect(),
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    let count = keys.len();
    let title = match count {
        0 => "License verifying keyring (empty)".to_string(),
        1 => "License verifying keyring (1 key)".to_string(),
        n => format!("License verifying keyring ({n} keys)"),
    };
    println!("{}", title.bold());
    println!("   Binary version:  {}", env!("CARGO_PKG_VERSION"));
    if keys.is_empty() {
        println!(
            "   {}",
            "No verifying keys embedded in this build.".yellow()
        );
    } else {
        println!("   Keys:");
        for key in &keys {
            println!("     {}", key.kid.cyan());
        }
    }
    println!();
    println!(
        "Ed25519 public keys embedded at compile time; a license must verify against one of these ids."
    );
    println!("Release packages may include more keys than a local `cargo build` from git alone.");
    Ok(())
}
