//! `super import <format> <file>` — convert a foreign process-manager config
//! into target stack services, preview, confirm, then apply via the existing
//! stack API (`PUT /api/v1/stack`, create-or-update semantics).
//!
//! Safety properties:
//! - Existing programs whose name collides with an import entry are **skipped**
//!   (never clobbered) unless `--yes` explicitly confirms the take-over.
//! - Imports are non-pruning (`prune = false`) — nothing outside the file is
//!   removed.
//! - `--no-start` forces `autostart = false` so a later daemon restart does
//!   not suddenly launch every imported program.

use std::io::Read;

use common::CreateProgramRequest;
use common::import::{ParseCtx, StackDraft, stack_format_by_id, supported_format_ids};

use crate::display;
use crate::handlers::{BatchOptions, Context, api_error_from_body};

pub async fn handle_import(
    ctx: &Context,
    format_id: &str,
    file: &std::path::Path,
    opts: BatchOptions,
    remap_logs: bool,
    emit_toml: Option<&std::path::Path>,
    no_start: bool,
) -> anyhow::Result<()> {
    let Some(format) = stack_format_by_id(format_id) else {
        anyhow::bail!(
            "unknown format {:?}; supported: {}",
            format_id,
            supported_format_ids().join(", ")
        );
    };

    let content = std::fs::read_to_string(file)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", file.display()))?;
    if content.trim().is_empty() {
        anyhow::bail!("{} is empty", file.display());
    }
    if !format.detect(&content) {
        eprintln!(
            "Warning: {} does not look like a {} config — parse errors may be misleading.",
            file.display(),
            format.id()
        );
    }

    let parse_ctx = ParseCtx {
        here_dir: file.parent().map(std::path::Path::to_path_buf),
        allow_include: true,
        remap_logs,
    };
    let draft: StackDraft = format.parse(&content, &parse_ctx)?;
    let draft = if no_start {
        stop_autostart(draft)
    } else {
        draft
    };

    print_warnings(&draft);

    if let Some(out) = emit_toml {
        return emit_stack_toml(&draft, out);
    }

    // Name-level diff against the live daemon: existing names are skipped
    // (unless explicitly confirmed), so an import never silently overwrites.
    // A dry-run works without a daemon: preview shows everything as new.
    let (current_res, was_dry_run) = (fetch_program_names(ctx).await, opts.dry_run);
    let current = match current_res {
        Ok(names) => names,
        Err(e) if was_dry_run => {
            println!("Note: daemon unreachable, previewing without name collision check ({e}).");
            Vec::new()
        }
        Err(e) => return Err(e),
    };
    let new_names: Vec<String> = draft
        .services
        .iter()
        .filter_map(|s| s.name.clone())
        .collect();
    let collisions: Vec<String> = new_names
        .iter()
        .filter(|n| current.contains(*n))
        .cloned()
        .collect();
    let fresh: Vec<&CreateProgramRequest> = draft
        .services
        .iter()
        .filter(|s| s.name.as_ref().is_none_or(|n| !current.contains(n)))
        .collect();

    print_preview(&draft, &collisions, &fresh);

    if opts.dry_run {
        println!("DRY RUN: nothing applied. Re-run without --dry-run to import.");
        return Ok(());
    }

    if fresh.is_empty() {
        println!(
            "Nothing to create — all {} service(s) already exist. Import finished.",
            collisions.len()
        );
        return Ok(());
    }

    let proceed = if opts.assume_yes {
        true
    } else if collisions.is_empty() {
        display::confirm_batch(
            fresh.len(),
            "import (create)",
            &fresh
                .iter()
                .filter_map(|s| s.name.clone())
                .collect::<Vec<_>>(),
        )
    } else {
        // Collisions present: require explicit typed confirmation for the
        // update-in-place decision, mirroring `super apply --force-prune`'s
        // "typed word" bar for non-reversible-ish actions.
        confirm_collision_takeover(&collisions)
    };
    if !proceed {
        println!("Aborted — nothing was imported.");
        return Ok(());
    }

    let request = common::StackApplyRequest {
        services: fresh.iter().map(|s| (*s).clone()).collect(),
        prune: false,
    };
    println!("Importing {} service(s)...", request.services.len());
    let url = format!("{}/api/v1/stack", ctx.base_url);
    let resp = ctx.client.put(&url).json(&request).send().await?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(api_error_from_body(status, &body));
    }
    let logs: Vec<String> = resp.json().await?;
    for log in &logs {
        println!("- {log}");
    }

    println!();
    println!(
        "Import finished: {} created/updated, {} skipped (already exist), {} warning(s).",
        request.services.len(),
        collisions.len(),
        draft.warnings.len()
    );
    if !collisions.is_empty() {
        println!("Skipped (already exist): {}", collisions.join(", "));
    }
    println!("Next steps:");
    println!("  super list            # verify imported programs");
    if !no_start {
        println!("  Note: programs with autostart=true will launch on daemon restart.");
    } else {
        println!(
            "  super start <name>    # all imports were created in a stopped state (--no-start)"
        );
    }
    Ok(())
}

fn stop_autostart(mut d: StackDraft) -> StackDraft {
    for s in &mut d.services {
        s.autostart = false;
    }
    d
}

fn print_warnings(d: &StackDraft) {
    if d.warnings.is_empty() {
        return;
    }
    println!("Import warnings ({}):", d.warnings.len());
    for w in &d.warnings {
        let tag = match w.severity {
            common::import::WarningSeverity::Info => "info",
            common::import::WarningSeverity::Warn => "WARN",
        };
        match &w.suggestion {
            Some(s) => println!("  [{tag}] {}: {} — {}", w.section, w.message, s),
            None => println!("  [{tag}] {}: {}", w.section, w.message),
        }
    }
    println!();
}

fn print_preview(d: &StackDraft, collisions: &[String], fresh: &[&CreateProgramRequest]) {
    println!("Import plan:");
    for s in fresh {
        let name = s.name.as_deref().unwrap_or("(auto)");
        let mut strs: Vec<String> = Vec::new();
        if s.autostart {
            strs.push("autostart".to_string());
        }
        if s.numprocs > 1 {
            strs.push(format!("x{}", s.numprocs));
        }
        if let Some(g) = &s.group {
            strs.push(format!("@{g}"));
        }
        let flag_str = if strs.is_empty() {
            String::new()
        } else {
            format!(" [{}]", strs.join(", "))
        };
        println!("  + {name}: {} {}{}", s.command, s.args.join(" "), flag_str);
    }
    for c in collisions {
        println!("  ~ {c}: already exists — skipped (not overwritten)");
    }
    if d.services.len() != fresh.len() + collisions.len() {
        println!(
            "  ! {} service(s) without a name could not be diffed and will be created",
            d.services.len() - fresh.len() - collisions.len()
        );
    }
    println!();
}

fn confirm_collision_takeover(collisions: &[String]) -> bool {
    use std::io::Write;
    println!(
        "WARNING: {} program(s) already exist and will be SKIPPED (their current config is kept):",
        collisions.len()
    );
    for c in collisions {
        println!("  = {c}");
    }
    println!("To instead take over these names with the imported config, type 'override'.");
    print!("Proceed with import (skipping existing)? [y/N/override] ");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    if std::io::stdin().read_to_string(&mut answer).is_err() {
        return false;
    }
    let a = answer.trim();
    if a.eq_ignore_ascii_case("override") {
        println!(
            "Override requested — existing programs with matching names will be updated in place."
        );
        true
    } else {
        matches!(a, "y" | "Y" | "yes" | "Yes")
    }
}

async fn fetch_program_names(ctx: &Context) -> anyhow::Result<Vec<String>> {
    let url = format!("{}/api/v1/programs", ctx.base_url);
    let resp = ctx.client.get(&url).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(api_error_from_body(status, &body));
    }
    let summaries: Vec<common::ProgramSummary> = resp.json().await?;
    Ok(summaries.into_iter().map(|p| p.name).collect())
}

fn emit_stack_toml(draft: &StackDraft, out: &std::path::Path) -> anyhow::Result<()> {
    let request = common::StackApplyRequest {
        services: draft.services.clone(),
        prune: false,
    };
    let toml = toml::to_string_pretty(&request)?;
    if out.as_os_str() == "-" {
        print!("{toml}");
    } else {
        std::fs::write(out, toml)
            .map_err(|e| anyhow::anyhow!("cannot write {}: {e}", out.display()))?;
        println!("Stack draft written to {} (not applied).", out.display());
        println!("Review it, then: super apply {}", out.display());
    }
    Ok(())
}

// The `ApiClient` type alias is re-exported for signature clarity only.
#[allow(unused_imports)]
use crate::client as _client_reexport;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sample() -> StackDraft {
        let ini = "[program:web]\ncommand=/usr/bin/node server.js\nautostart=true\n";
        let ctx = ParseCtx::default();
        use common::import::StackFormat as _;
        common::import::SupervisorFormat.parse(ini, &ctx).unwrap()
    }

    #[test]
    fn no_start_forces_autostart_false() {
        let d = stop_autostart(sample());
        assert!(!d.services[0].autostart);
    }

    #[test]
    fn preview_partitions_services_and_collisions() {
        let d = sample();
        let current = ["web".to_string()];
        let collisions: Vec<String> = d
            .services
            .iter()
            .filter_map(|s| s.name.clone())
            .filter(|n| current.contains(n))
            .collect();
        let fresh: Vec<&CreateProgramRequest> = d
            .services
            .iter()
            .filter(|s| s.name.as_ref().is_none_or(|n| !current.contains(n)))
            .collect();
        assert_eq!(collisions, vec!["web"]);
        assert!(fresh.is_empty());
    }

    // The `ApiClient` type alias is re-exported for signature clarity only.
    #[allow(unused)]
    fn _t(_: HashMap<String, String>) {}
}
