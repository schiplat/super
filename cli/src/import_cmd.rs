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

use common::CreateProgramRequest;
use common::import::{ParseCtx, StackDraft, stack_format_by_id, supported_format_ids};

use crate::args::CollisionMode;
use crate::display;
use crate::handlers::{BatchOptions, Context, api_error_from_body};

#[allow(clippy::too_many_arguments)]
pub async fn handle_import(
    ctx: &Context,
    format_id: &str,
    file: &std::path::Path,
    opts: BatchOptions,
    remap_logs: bool,
    emit_toml: Option<&std::path::Path>,
    no_start: bool,
    mut on_collision: CollisionMode,
    collision_suffix: Option<&str>,
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
    let mut draft = if no_start {
        stop_autostart(draft)
    } else {
        draft
    };
    // Provenance: every service created/updated by this import carries the
    // source format label (e.g. `import:supervisor`).
    for svc in &mut draft.services {
        if svc.source.is_none() {
            svc.source = Some(format!("import:{format_id}"));
        }
    }

    print_warnings(&draft);

    if let Some(out) = emit_toml {
        return emit_stack_toml(&draft, out);
    }

    // Name-level diff against the live daemon: collisions get the
    // `--on-collision` treatment (skip / rename / override). A dry-run works
    // without a daemon: preview shows everything as new.
    let (current_res, was_dry_run) = (fetch_program_names(ctx).await, opts.dry_run);
    let current = match current_res {
        Ok(names) => names,
        Err(e) if was_dry_run => {
            println!("Note: daemon unreachable, previewing without name collision check ({e}).");
            Vec::new()
        }
        Err(e) => return Err(e),
    };

    // Resolve collisions BEFORE preview so dry-run, emit-toml and the real
    // apply all show the exact same names (renames included).
    let (mut fresh, mut renamed_map, mut collisions): (
        Vec<CreateProgramRequest>,
        Vec<(String, String)>,
        Vec<String>,
    ) = resolve_collisions(&draft, &current, on_collision, collision_suffix);

    // Interactive confirmation may still change the mode when the flag was
    // left at the default `skip`: offer rename/override at the prompt.
    if on_collision == CollisionMode::Skip && !collisions.is_empty() && !opts.assume_yes {
        match confirm_collision_mode(&collisions)? {
            CollisionPrompt::KeepSkip => {}
            CollisionPrompt::Switch(mode) => {
                on_collision = mode;
                let (f, r, s) =
                    resolve_collisions(&draft, &current, on_collision, collision_suffix);
                fresh = f;
                renamed_map = r;
                collisions = s;
            }
            CollisionPrompt::Abort => {
                println!("Aborted — nothing was imported.");
                return Ok(());
            }
        }
    }

    print_preview(&draft, &collisions, &fresh, &renamed_map, on_collision);

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
    } else if on_collision == CollisionMode::Override && !collisions.is_empty() {
        // Override of existing names is a real in-place update: keep the
        // "typed word" bar, mirroring `super apply --force-prune` for
        // non-reversible-ish actions. Must be checked BEFORE the generic
        // no-collision branch — in override mode collided services are part
        // of `fresh`, so `collisions` carries the in-place-update names.
        confirm_override(&collisions)
    } else if renamed_map.is_empty() {
        display::confirm_batch(
            fresh.len(),
            "import (create)",
            &fresh
                .iter()
                .filter_map(|s| s.name.clone())
                .collect::<Vec<_>>(),
        )
    } else {
        // Rename mode: new names never touch existing programs, but show the
        // mapping and get a plain yes/no.
        println!("Renamed imports (existing programs stay untouched):");
        for (from, to) in &renamed_map {
            println!("  {from} -> {to}");
        }
        display::confirm_batch(
            fresh.len(),
            "import (rename)",
            &fresh
                .iter()
                .filter_map(|s| s.name.clone())
                .collect::<Vec<_>>(),
        )
    };
    if !proceed {
        println!("Aborted — nothing was imported.");
        return Ok(());
    }

    // TOCTOU guard: the name snapshot was taken before the preview/confirm
    // prompts. Re-check now so a program created in the meantime cannot turn
    // a promised create into an in-place update via stack apply's
    // create-or-update semantics. (Override mode updates in place by
    // definition; rename targets are unguessable.)
    if on_collision != CollisionMode::Override {
        let now_names = fetch_program_names(ctx).await?;
        let raced: Vec<String> = fresh
            .iter()
            .filter_map(|s| s.name.clone())
            .filter(|n| now_names.contains(n))
            .collect();
        if let Some(first) = raced.first() {
            return Err(anyhow::anyhow!(
                "program '{first}' appeared on the daemon while the import was \
                 being confirmed (name changed state between check and apply). \
                 Nothing was imported — re-run `super import` to see the \
                 current state and decide again."
            ));
        }
    }

    let request = common::StackApplyRequest {
        services: fresh.clone(),
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
    if !renamed_map.is_empty() {
        println!("Renamed on collision:");
        for (from, to) in &renamed_map {
            println!("  {from} -> {to}");
        }
    }
    match on_collision {
        CollisionMode::Override if !collisions.is_empty() => {
            println!(
                "Import finished: {} created/updated ({} overridden in place), {} warning(s).",
                request.services.len(),
                collisions.len(),
                draft.warnings.len()
            );
        }
        _ => {
            println!(
                "Import finished: {} created/updated, {} skipped (already exist), {} warning(s).",
                request.services.len(),
                collisions.len(),
                draft.warnings.len()
            );
            if !collisions.is_empty() {
                println!("Skipped (already exist): {}", collisions.join(", "));
            }
        }
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

/// What to do with names that already exist on the daemon.
///
/// Returns the modified service list (`fresh`), the rename map (old -> new)
/// and the names skipped under the chosen mode.
fn resolve_collisions(
    draft: &StackDraft,
    current: &[String],
    mode: CollisionMode,
    suffix: Option<&str>,
) -> (
    Vec<CreateProgramRequest>,
    Vec<(String, String)>,
    Vec<String>,
) {
    let mut fresh = Vec::new();
    let mut renamed = Vec::new();
    let mut skipped = Vec::new();
    // Names already claimed inside this batch (daemon + earlier services +
    // earlier renames) so two services never resolve to the same target.
    let mut taken: Vec<String> = current.to_vec();
    if mode == CollisionMode::Rename {
        // A rename target must also avoid names claimed by LATER services
        // in this batch (they pass through untouched): with a fixed suffix,
        // `web` must not rename onto a file sibling named `web-staging`.
        for svc in &draft.services {
            if let Some(name) = &svc.name {
                taken.push(name.clone());
            }
        }
    }

    for svc in &draft.services {
        let Some(name) = &svc.name else {
            fresh.push(svc.clone());
            continue;
        };
        if current.contains(name) {
            match mode {
                CollisionMode::Skip => {
                    skipped.push(name.clone());
                    continue;
                }
                CollisionMode::Rename => {
                    let new_name = pick_renamed(name, &taken, suffix);
                    // Register the target so a later duplicate source name
                    // (or fixed-suffix re-run within the batch) stays unique.
                    // (The source name itself is already in `taken`.)
                    taken.push(new_name.clone());
                    renamed.push((name.clone(), new_name.clone()));
                    let mut svc2 = svc.clone();
                    svc2.name = Some(new_name);
                    fresh.push(svc2);
                    continue;
                }
                CollisionMode::Override => {
                    // Keep the original name; stack apply updates in place.
                    fresh.push(svc.clone());
                    continue;
                }
            }
        }
        taken.push(name.clone());
        fresh.push(svc.clone());
    }
    // Override mode: collided names were folded into `fresh` above; report
    // them so the typed confirmation lists exactly what gets updated.
    if mode == CollisionMode::Override {
        for svc in &draft.services {
            if let Some(name) = &svc.name
                && current.contains(name)
            {
                skipped.push(name.clone());
            }
        }
    }
    skipped.sort();
    skipped.dedup();
    (fresh, renamed, skipped)
}

/// Generate a collision-free `{name}-{suffix}` name. Uses a random 6-char
/// lowercase-hex suffix by default; `suffix` pins it (scripted reproducibility).
fn pick_renamed(base: &str, taken: &[String], suffix: Option<&str>) -> String {
    if let Some(s) = suffix {
        let candidate = format!("{base}-{s}");
        if !taken.iter().any(|t| t == &candidate) {
            return candidate;
        }
        // Fixed suffix taken: fall through to random to stay collision-free.
    }
    use rand::Rng as _;
    let mut rng = rand::thread_rng();
    loop {
        let hex: String = (0..6)
            .map(|_| format!("{:x}", rng.gen_range(0..16)))
            .collect();
        let candidate = format!("{base}-{hex}");
        if !taken.iter().any(|t| t == &candidate) {
            return candidate;
        }
    }
}

/// Outcome of the interactive collision prompt (only called when `!assume_yes`
/// and collisions are non-empty).
enum CollisionPrompt {
    /// User answered `y` — keep the default skip behaviour.
    KeepSkip,
    /// User switched modes at the prompt (`r` / `o`).
    Switch(CollisionMode),
    /// User aborted (`N` / empty / EOF).
    Abort,
}

fn confirm_collision_mode(skipped: &[String]) -> anyhow::Result<CollisionPrompt> {
    use std::io::Write;
    println!(
        "WARNING: {} program(s) already exist and will be SKIPPED (their current config is kept):",
        skipped.len()
    );
    for s in skipped {
        println!("  = {s}");
    }
    println!(
        "Options: [y] proceed skipping these, [r] import renamed side-by-side, [o] override existing, [N] abort"
    );
    print!("Proceed? [y/r/o/N] ");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    let n = std::io::stdin().read_line(&mut answer)?;
    if n == 0 {
        // EOF (piped/closed stdin): fail closed like the other prompts.
        println!();
        return Ok(CollisionPrompt::Abort);
    }
    Ok(match answer.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => CollisionPrompt::KeepSkip,
        "r" | "rename" => CollisionPrompt::Switch(CollisionMode::Rename),
        "o" | "override" => CollisionPrompt::Switch(CollisionMode::Override),
        _ => CollisionPrompt::Abort,
    })
}

/// Typed confirmation for the destructive-ish override mode.
fn confirm_override(collisions: &[String]) -> bool {
    use std::io::Write;
    println!(
        "WARNING: {} existing program(s) will be UPDATED IN PLACE with the imported config:",
        collisions.len()
    );
    for c in collisions {
        println!("  ! {c}");
    }
    println!("Their current configuration will be replaced. This cannot be undone.");
    print!("Type 'override' to confirm, anything else to abort: ");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    let a = answer.trim();
    if a.eq_ignore_ascii_case("override") {
        println!("Override confirmed — existing programs will be updated in place.");
        true
    } else {
        false
    }
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

fn print_preview(
    _draft: &StackDraft,
    collisions: &[String],
    fresh: &[CreateProgramRequest],
    renamed: &[(String, String)],
    mode: CollisionMode,
) {
    println!("Import plan:");
    let overridden: std::collections::HashSet<&str> =
        collisions.iter().map(String::as_str).collect();
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
        let marker = if overridden.contains(name) { "~" } else { "+" };
        println!(
            "  {marker} {name}: {} {}{}",
            s.command,
            s.args.join(" "),
            flag_str
        );
    }
    for (from, to) in renamed {
        println!("  ~ {from}: exists -> will import as {to} (renamed)");
    }
    for c in collisions {
        let note = if mode == CollisionMode::Override {
            "exists -> will UPDATE IN PLACE"
        } else {
            "already exists — skipped (not overwritten)"
        };
        println!("  ~ {c}: {note}");
    }
    println!();
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
        let (fresh, renamed, skipped) = resolve_collisions(&d, &current, CollisionMode::Skip, None);
        assert_eq!(skipped, vec!["web".to_string()]);
        assert!(fresh.is_empty());
        assert!(renamed.is_empty());
    }

    #[test]
    fn rename_mode_generates_unique_suffixed_names() {
        let d = sample(); // program:web
        let current = ["web".to_string()];
        let (fresh, renamed, skipped) =
            resolve_collisions(&d, &current, CollisionMode::Rename, None);
        assert!(skipped.is_empty());
        assert_eq!(renamed.len(), 1);
        let (from, to) = &renamed[0];
        assert_eq!(from, "web");
        assert!(to.starts_with("web-") && to.len() == "web-XXXXXX".len());
        assert_ne!(fresh[0].name.as_deref(), Some("web"));
        assert_eq!(fresh[0].name.as_deref(), Some(to.as_str()));
    }

    #[test]
    fn rename_with_fixed_suffix_and_fallback() {
        let d = sample();
        let current = ["web".to_string()];
        let (_, renamed, _) =
            resolve_collisions(&d, &current, CollisionMode::Rename, Some("staging"));
        assert_eq!(renamed[0].1, "web-staging");
        // Fixed suffix already taken on the daemon -> falls back to random.
        let current2 = ["web".to_string(), "web-staging".to_string()];
        let (_, renamed2, _) =
            resolve_collisions(&d, &current2, CollisionMode::Rename, Some("staging"));
        assert_eq!(renamed2[0].0, "web");
        assert_ne!(renamed2[0].1, "web-staging");
        assert!(renamed2[0].1.starts_with("web-"));
    }

    #[test]
    fn override_mode_keeps_name_and_includes_service() {
        // The override bug: collided services must stay IN the apply set
        // AND be reported as the names being overridden.
        let d = sample(); // program:web collides
        let current = ["web".to_string()];
        let (fresh, renamed, skipped) =
            resolve_collisions(&d, &current, CollisionMode::Override, None);
        assert!(renamed.is_empty());
        assert_eq!(skipped, vec!["web".to_string()], "override names reported");
        assert_eq!(fresh.len(), 1, "override keeps service in apply set");
        assert_eq!(fresh[0].name.as_deref(), Some("web"));
    }

    #[test]
    fn non_colliding_services_pass_through_all_modes() {
        let d = sample(); // web
        let current = ["other".to_string()];
        for mode in [
            CollisionMode::Skip,
            CollisionMode::Rename,
            CollisionMode::Override,
        ] {
            let (fresh, renamed, skipped) = resolve_collisions(&d, &current, mode, None);
            assert_eq!(fresh.len(), 1, "mode {mode:?}");
            assert_eq!(fresh[0].name.as_deref(), Some("web"));
            assert!(renamed.is_empty());
            assert!(skipped.is_empty());
        }
    }

    #[test]
    fn duplicate_names_within_batch_rename_consistently() {
        let d = StackDraft {
            services: vec![svc_named("web"), svc_named("web")],
            warnings: Vec::new(),
        };
        let current: Vec<String> = vec![];
        // No daemon collision: both pass through untouched.
        let (fresh, _, _) = resolve_collisions(&d, &current, CollisionMode::Skip, None);
        assert_eq!(fresh.len(), 2);
        // Rename with a daemon collision: batch-internal names stay unique.
        let current2 = ["web".to_string()];
        let (fresh2, renamed2, _) =
            resolve_collisions(&d, &current2, CollisionMode::Rename, Some("x"));
        assert_eq!(renamed2.len(), 2);
        assert_ne!(renamed2[0].1, renamed2[1].1, "renamed targets must differ");
        let names: Vec<_> = fresh2.iter().filter_map(|s| s.name.clone()).collect();
        assert_ne!(names[0], names[1]);
    }

    #[test]
    fn rename_target_avoids_later_batch_sibling() {
        // Regression: file = [web, web-staging], daemon = [web]. With a fixed
        // suffix "staging" the renamed target for `web` would have been
        // `web-staging` — clobbering the sibling `web-staging` service that
        // passes through untouched. The target must fall back to random.
        let d = StackDraft {
            services: vec![svc_named("web"), svc_named("web-staging")],
            warnings: Vec::new(),
        };
        let current = ["web".to_string()];
        let (fresh, renamed, skipped) =
            resolve_collisions(&d, &current, CollisionMode::Rename, Some("staging"));
        assert_eq!(skipped, Vec::<String>::new());
        assert_eq!(renamed.len(), 1, "only `web` collides with the daemon");
        assert_eq!(renamed[0].0, "web");
        assert_ne!(
            renamed[0].1, "web-staging",
            "rename target must not equal the batch sibling name"
        );
        let names: Vec<_> = fresh.iter().filter_map(|s| s.name.clone()).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"web-staging".to_string()));
    }

    fn svc_named(name: &str) -> CreateProgramRequest {
        CreateProgramRequest {
            name: Some(name.to_string()),
            command: "/bin/sleep".to_string(),
            args: vec!["1".to_string()],
            retry_limit: 3,
            startsecs: 10,
            exitcodes: vec![0],
            numprocs: 1,
            ..Default::default()
        }
    }

    // The `ApiClient` type alias is re-exported for signature clarity only.
    #[allow(unused)]
    fn _t(_: HashMap<String, String>) {}
}
