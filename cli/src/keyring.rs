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
        0 => "License root keyring (empty)".to_string(),
        1 => "License root keyring (1 key)".to_string(),
        n => format!("License root keyring ({n} keys)"),
    };
    println!("{}", title.bold());
    println!("   Binary version:  {}", env!("CARGO_PKG_VERSION"));
    if keys.is_empty() {
        println!("   {}", "No root keys embedded in this build.".yellow());
    } else {
        println!("   Root keys:");
        for key in &keys {
            println!("     {}", key.kid.cyan());
        }
    }
    println!();
    println!(
        "Root Ed25519 public keys embedded at compile time; every license must carry a \
         signing-key certificate signed by one of them."
    );
    println!("Signing-key rotation does not require a new build; root rotation does.");
    Ok(())
}
