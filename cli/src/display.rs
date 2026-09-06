use comfy_table::{Cell, Color, Table, presets::UTF8_FULL};
use common::{AutorestartPolicy, HealthCheck, ProcessStatus, ProgramInfo, ProgramSummary};
use std::io::Write;

// Secret masking filter
fn mask_secret(key: &str, value: &str) -> String {
    let k = key.to_uppercase();
    if k.contains("SECRET")
        || k.contains("PASSWORD")
        || k.contains("TOKEN")
        || k.contains("KEY")
        || k.contains("CREDENTIAL")
    {
        "********".to_string()
    } else {
        value.to_string()
    }
}

const PREVIEW_LIMIT: usize = 20;

/// Split a program-name preview into (lines to print, hidden count). Both the
/// interactive confirmation and `--dry-run` share this truncation so a large
/// fleet cannot flood the terminal and the two previews always agree.
fn preview_split(names: &[String]) -> (Vec<&str>, usize) {
    let shown = names.len().min(PREVIEW_LIMIT);
    let lines: Vec<&str> = names.iter().take(shown).map(String::as_str).collect();
    let hidden = names.len().saturating_sub(PREVIEW_LIMIT);
    (lines, hidden)
}

/// Confirm a multi-program batch operation. `preview` lists the affected
/// program names so the user sees exactly what would be touched.
///
/// `count <= 1` never prompts (single-program operations are the norm and the
/// target is already explicit). Otherwise the user must type `y`/`yes`.
pub fn confirm_batch(count: usize, action: &str, preview: &[String]) -> bool {
    if count <= 1 {
        return true;
    }

    println!("WARNING: You are about to {} {} programs.", action, count);
    let (lines, hidden) = preview_split(preview);
    for name in lines {
        println!("   - {}", name);
    }
    if hidden > 0 {
        println!("   ... and {} more", hidden);
    }
    print!("Are you sure you want to continue? [y/N] ");
    let _ = std::io::stdout().flush();

    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_ok() {
        let t = input.trim().to_lowercase();
        return t == "y" || t == "yes";
    }
    false
}

/// Print the programs a dry-run batch operation would affect. Truncated to
/// [`PREVIEW_LIMIT`] like the interactive confirmation, so a large fleet
/// cannot flood the terminal.
pub fn print_dry_run(action: &str, target: &str, names: &[String]) {
    println!(
        "DRY RUN: would {} {} program(s) for target '{}':",
        action,
        names.len(),
        target
    );
    let (lines, hidden) = preview_split(names);
    for name in lines {
        println!("   - {}", name);
    }
    if hidden > 0 {
        println!("   ... and {} more", hidden);
    }
}

/// Exact string the operator must type to confirm an irreversible prune.
pub const PRUNE_CONFIRM_TOKEN: &str = "confirmed";

/// Planned create / keep / remove sets for a stack apply (name-level diff).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyPlan {
    /// In the stack file, not currently managed — will be created.
    pub create: Vec<String>,
    /// In both the stack and the daemon — will be created-or-updated (upsert).
    pub keep: Vec<String>,
    /// Currently managed, missing from the stack — removed only when `prune = true`.
    pub remove: Vec<String>,
}

/// Build a sorted name-level apply plan from stack inventory vs current programs.
pub fn build_apply_plan(
    stack_names: impl IntoIterator<Item = String>,
    current_names: impl IntoIterator<Item = String>,
    prune: bool,
) -> ApplyPlan {
    use std::collections::HashSet;
    let stack: HashSet<String> = stack_names.into_iter().collect();
    let current: HashSet<String> = current_names.into_iter().collect();

    let mut create: Vec<String> = stack.difference(&current).cloned().collect();
    let mut keep: Vec<String> = stack.intersection(&current).cloned().collect();
    let mut remove: Vec<String> = if prune {
        current.difference(&stack).cloned().collect()
    } else {
        Vec::new()
    };
    create.sort();
    keep.sort();
    remove.sort();
    ApplyPlan {
        create,
        keep,
        remove,
    }
}

fn eprint_name_section(title: &str, names: &[String], truncate: bool) {
    eprintln!("{} ({}):", title, names.len());
    if names.is_empty() {
        eprintln!("   (none)");
        return;
    }
    if truncate {
        let (lines, hidden) = preview_split(names);
        for name in lines {
            eprintln!("   - {}", name);
        }
        if hidden > 0 {
            eprintln!("   ... and {} more", hidden);
        }
    } else {
        // Removals are irreversible — never hide a victim name.
        for name in names {
            eprintln!("   - {}", name);
        }
    }
}

/// Print the apply diff to stderr. Removal names are listed in full (no truncation).
pub fn print_apply_diff(file: &str, prune: bool, plan: &ApplyPlan) {
    eprintln!();
    eprintln!("Apply diff for {} (prune={}):", file, prune);
    eprint_name_section("  keep/update", &plan.keep, true);
    eprint_name_section("  create", &plan.create, true);
    if prune {
        eprint_name_section(
            "  REMOVE (stop + unregister — irreversible)",
            &plan.remove,
            false,
        );
    } else {
        eprintln!("  remove: skipped (prune=false)");
    }
    eprintln!();
}

/// Returns true if `input` (typically a read line) accepts prune confirmation.
pub fn prune_confirmation_accepted(input: &str) -> bool {
    input.trim() == PRUNE_CONFIRM_TOKEN
}

/// Confirm a stack apply that requested `prune=true` and would remove programs.
///
/// Prints the full apply diff first (create / keep / **every** removal), then
/// requires typing **`confirmed`** exactly ([`PRUNE_CONFIRM_TOKEN`]).
/// Unlike [`confirm_batch`] (`y`/`yes`), soft affirmatives are rejected.
/// Returns `true` when there is nothing to prune.
pub fn confirm_prune(file: &str, plan: &ApplyPlan) -> bool {
    if plan.remove.is_empty() {
        return true;
    }

    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════════════════╗");
    eprintln!("║  IRREVERSIBLE: prune = true                                      ║");
    eprintln!("╚══════════════════════════════════════════════════════════════════╝");
    eprintln!(
        "Super will STOP and REMOVE {} managed program(s) that are NOT in this stack.",
        plan.remove.len()
    );
    eprintln!(
        "This cannot be undone from Super (you must recreate programs / re-apply a full stack)."
    );
    eprintln!("External DB/files are not deleted — but removed process definitions are gone.");
    print_apply_diff(file, true, plan);
    eprintln!("Anything other than the confirmation word aborts (y / yes / prune are NOT enough).");
    eprintln!();
    // Make the required token unmistakable: own line + brackets + bold/bright color.
    {
        use colored::Colorize;
        eprintln!("Type this confirmation word exactly (lowercase, no quotes):");
        eprintln!();
        eprintln!("    >>> {} <<<", PRUNE_CONFIRM_TOKEN.bold().bright_yellow());
        eprintln!();
        eprint!("{} ", "Confirmation:".bold().bright_yellow());
    }
    let _ = std::io::stderr().flush();

    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_ok() {
        return prune_confirmation_accepted(&input);
    }
    false
}

pub fn print_list_table(mut programs: Vec<ProgramSummary>) {
    programs.sort_by(|a, b| {
        let group_a = a.group.as_deref().unwrap_or("");
        let group_b = b.group.as_deref().unwrap_or("");
        match group_a.cmp(group_b) {
            std::cmp::Ordering::Equal => a.name.cmp(&b.name),
            other => other,
        }
    });

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "ID", "Name", "Group", "Status", "PID", "CPU", "Mem", "Uptime", "Updated",
    ]);

    for p in programs {
        // Status display logic
        // Prefer ProcessStatus; detail view can be richer.
        // Summary omits config.restore_path to save bandwidth,
        // so this shows runtime state only. For "OTA-Verifying",
        // map status on the server or add a flag to Summary.
        // Note: restore_path is not on Summary; extend common/ProgramSummary to show it in list.
        //
        // Keep list view as-is; show details in info.

        let status_color = match p.status {
            ProcessStatus::Healthy => Color::Green,
            ProcessStatus::Running => Color::Green, // lighter green
            ProcessStatus::Stopped => Color::Grey,
            ProcessStatus::Fatal => Color::Red,
            ProcessStatus::Backoff => Color::Yellow,
            ProcessStatus::Waiting => Color::Blue,
            _ => Color::White,
        };

        let updated_str = if p.updated_at > 0 {
            chrono::DateTime::from_timestamp(p.updated_at as i64, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| p.updated_at.to_string())
        } else {
            "-".to_string()
        };

        let pid_str = p
            .pid
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string());
        let group_str = p.group.as_deref().unwrap_or("-");
        let cpu_str = p
            .cpu_usage
            .map(|v| format!("{:.1}%", v))
            .unwrap_or("-".to_string());
        let mem_str = p
            .mem_usage
            .map(|v| {
                const MB: u64 = 1024 * 1024;
                if v > MB {
                    format!("{:.1} MB", v as f64 / MB as f64)
                } else {
                    format!("{} KB", v / 1024)
                }
            })
            .unwrap_or("-".to_string());

        table.add_row(vec![
            Cell::new(p.id.to_string().split_at(8).0.to_string()),
            Cell::new(p.name.clone()).fg(Color::Cyan),
            Cell::new(group_str),
            Cell::new(format!("{:?}", p.status)).fg(status_color),
            Cell::new(pid_str),
            Cell::new(cpu_str),
            Cell::new(mem_str),
            Cell::new(p.uptime_sec.map(|s| format!("{}s", s)).unwrap_or_default()),
            Cell::new(updated_str),
        ]);
    }
    println!("{table}");
}

pub fn print_info(info: ProgramInfo) {
    println!("--- Program Details ---");
    println!("ID:        {}", info.id);
    println!("Name:      {}", info.config.name);
    if let Some(g) = &info.config.group {
        println!("Group:     {}", g);
    }

    let full_cmd = std::iter::once(info.config.command.clone())
        .chain(info.config.args.iter().map(|arg| {
            if arg.contains(' ') {
                format!("\"{}\"", arg)
            } else {
                arg.clone()
            }
        }))
        .collect::<Vec<_>>()
        .join(" ");
    println!("Full Cmd:  {}", full_cmd);

    println!(
        "CWD:       {}",
        info.config.cwd.unwrap_or_else(|| "(default)".to_string())
    );
    if let Some(u) = &info.config.user {
        println!("User:      {}", u);
    }
    if let Some(cron) = &info.config.cron {
        println!("Cron:      {}", cron);
        if let Some(o) = &info.config.on_overlap {
            println!("Overlap:   {:?}", o);
        }
        if let Some(c) = &info.config.catchup {
            println!("Catchup:   {:?}", c);
        }
        if let Some(j) = info.config.jitter_sec {
            println!("Jitter:    {}s", j);
        }
        if let Some(mc) = info.config.max_concurrent {
            println!("MaxConc:   {}", mc);
        }
        if let Some(mq) = info.config.max_queued {
            println!("MaxQueued: {}", mq);
        }
    }

    // Print env file reference
    if let Some(env_file) = &info.config.env_file {
        println!("Env File:  {}", env_file);
    }

    // OTA status display
    // restore_path set means an upgrade verification transaction is active
    if let Some(bak) = &info.config.restore_path {
        println!("Upgrade:   VERIFYING (Transaction Active)");
        println!("Backup:    {}", bak);
    }

    if let Some(hc) = &info.config.health_check {
        let tuning = format!(
            " [every {}s, timeout {}s, start {}s, restart after {}x]",
            hc.interval_secs(),
            hc.timeout_secs(),
            hc.start_period_secs(),
            hc.max_failures()
        );
        match hc {
            HealthCheck::Tcp { port, host, .. } => {
                println!("Health:    TCP {}:{}{}", host, port, tuning)
            }
            HealthCheck::Http { url, method, .. } => println!(
                "Health:    HTTP {} {}{}",
                method.as_deref().unwrap_or("GET"),
                url,
                tuning
            ),
            HealthCheck::Exec { command, .. } => {
                println!("Health:    EXEC '{}'{}", command, tuning)
            }

            // Server usually maps Disabled to None; edge cases may still see it
            HealthCheck::Disabled => println!("Health:    Disabled (Pending Removal)"),
        }
    }

    if !info.config.depends_on.is_empty() {
        println!("Depends:   {:?}", info.config.depends_on);
    }

    if !info.config.env.is_empty() {
        println!("Environment:");
        let mut env_vec: Vec<_> = info.config.env.iter().collect();
        env_vec.sort_by_key(|(k, _)| *k);
        for (k, v) in env_vec {
            println!("  {}={}", k, mask_secret(k, v));
        }
    }
    println!("-----------------------");
    println!("State:     {:?}", info.state);
    if let Some(pid) = info.pid {
        println!("PID:       {}", pid);
    }
    println!("Autostart:  {}", info.config.autostart);
    let ar = match info.config.autorestart {
        AutorestartPolicy::Unexpected => "unexpected",
        AutorestartPolicy::True => "true",
        AutorestartPolicy::False => "false",
    };
    println!("Autorestart: {}", ar);
    println!("Exitcodes:  {:?}", info.config.exitcodes);
    println!("Startsecs:  {}s", info.config.startsecs);
    if let Some(secs) = info.config.stopsecs {
        println!("Stopsecs:   {}s", secs);
    }

    if let Some(limits) = &info.config.resource_limits {
        println!("Resources:");
        if let Some(cpu) = limits.cpu_quota {
            println!("  CPU Quota: {:.2} cores", cpu);
        }
        if let Some(mem) = limits.memory_limit {
            println!("  Mem Limit: {} MB", mem);
        }
        if let Some(w) = limits.memory_warn_percent {
            println!("  Mem Warn:  {}% of limit", w);
        }
        if let Some(h) = limits.memory_warn_headroom {
            println!("  Mem Warn Headroom: {} MB", h);
        }
        if let Some(high) = limits.memory_high {
            println!("  Mem High (soft): {} MB", high);
        }
    }

    if let Some(art) = &info.config.artifact {
        println!("Artifact:");
        println!("  Source:  {}", art.source);
        println!("  Dest:    {}", art.destination);
        println!("  Extract: {}", art.extract);
        println!("  Restart: {}", art.restart_policy);
        println!("  Download timeout: {}s", art.download_timeout);
        println!("  Verify timeout:   {}s", art.verify_timeout);
    }
}

pub fn print_events(events: &[common::ProgramEventRecord], limit: Option<usize>) {
    if events.is_empty() {
        println!("No lifecycle events recorded for this program.");
        return;
    }

    let iter: Box<dyn Iterator<Item = &common::ProgramEventRecord>> = match limit {
        Some(n) => Box::new(events.iter().rev().take(n)),
        None => Box::new(events.iter().rev()),
    };

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Time", "Event", "Exit", "Signal", "Retry", "Message"]);

    for e in iter {
        let ts = chrono::DateTime::from_timestamp(e.ts as i64, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| e.ts.to_string());
        let event_color = match e.event.as_str() {
            "process_fatal" => Color::Red,
            "process_backoff" => Color::Yellow,
            "process_recovered" => Color::Green,
            _ => Color::White,
        };
        table.add_row(vec![
            Cell::new(ts),
            Cell::new(e.event.clone()).fg(event_color),
            Cell::new(
                e.exit_code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            ),
            Cell::new(
                e.signal
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            ),
            Cell::new(
                e.retry_count
                    .map(|r| r.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            ),
            Cell::new(e.msg.clone()),
        ]);
    }
    println!("{table}");
}

pub fn print_event_stats(stats: &common::EventStats) {
    println!("Total events: {}", stats.total);
    if let Some(first) = stats.first_ts {
        let d = chrono::DateTime::from_timestamp(first as i64, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| first.to_string());
        println!("First event: {d}");
    }
    if let Some(last) = stats.last_ts {
        let d = chrono::DateTime::from_timestamp(last as i64, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| last.to_string());
        println!("Last event:  {d}");
    }

    if stats.by_type.is_empty() {
        println!("No events by type.");
        return;
    }
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Event", "Count"]);
    for t in &stats.by_type {
        table.add_row(vec![t.event.clone(), t.count.to_string()]);
    }
    println!("{table}");
}

pub fn print_token_table(tokens: Vec<common::AuthTokenInfo>) {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["ID", "Name", "Prefix", "Role", "Created"]);

    for t in tokens {
        let d = chrono::DateTime::from_timestamp(t.created_at as i64, 0)
            .unwrap_or_default()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        table.add_row(vec![
            t.id,
            t.name,
            format!("{}***", t.token_prefix),
            format!("{:?}", t.role),
            d,
        ]);
    }
    println!("{table}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn confirm_prune_skips_prompt_when_empty() {
        let plan = ApplyPlan {
            create: vec![],
            keep: names(&["web"]),
            remove: vec![],
        };
        assert!(confirm_prune("stack.toml", &plan));
    }

    #[test]
    fn build_apply_plan_classifies_create_keep_remove() {
        let plan = build_apply_plan(
            names(&["web", "worker", "beat"]),
            names(&["web", "legacy", "old-nginx"]),
            true,
        );
        assert_eq!(plan.create, names(&["beat", "worker"]));
        assert_eq!(plan.keep, names(&["web"]));
        assert_eq!(plan.remove, names(&["legacy", "old-nginx"]));

        let no_prune = build_apply_plan(names(&["web"]), names(&["web", "legacy"]), false);
        assert!(no_prune.remove.is_empty());
        assert_eq!(no_prune.keep, names(&["web"]));
    }

    #[test]
    fn prune_confirmation_requires_exact_token() {
        assert!(prune_confirmation_accepted("confirmed"));
        assert!(prune_confirmation_accepted("  confirmed\n"));
        assert!(!prune_confirmation_accepted("y"));
        assert!(!prune_confirmation_accepted("yes"));
        assert!(!prune_confirmation_accepted("prune"));
        assert!(!prune_confirmation_accepted("CONFIRMED"));
        assert!(!prune_confirmation_accepted("confirmed please"));
    }

    #[test]
    fn confirm_batch_never_prompts_for_single() {
        assert!(confirm_batch(0, "STOP", &[]));
        let one = vec!["web".to_string()];
        assert!(confirm_batch(1, "STOP", &one));
    }

    #[test]
    fn confirm_batch_aborts_without_stdin() {
        // In a test binary stdin is not a TTY and there is no input; the
        // fail-closed path returns false.
        let names: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        assert!(!confirm_batch(3, "STOP", &names));
    }

    #[test]
    fn confirm_batch_accepts_empty_preview() {
        // count > 1 with no preview still prompts (old behaviour).
        assert!(!confirm_batch(3, "RESTART", &[]));
    }

    #[test]
    fn preview_split_shows_everything_under_limit() {
        let src = names(&["a", "b"]);
        let (lines, hidden) = preview_split(&src);
        assert_eq!(lines, vec!["a", "b"]);
        assert_eq!(hidden, 0);
    }

    #[test]
    fn preview_split_truncates_at_limit() {
        let many: Vec<String> = (0..PREVIEW_LIMIT + 7).map(|i| format!("p{i}")).collect();
        let (lines, hidden) = preview_split(&many);
        assert_eq!(lines.len(), PREVIEW_LIMIT);
        assert_eq!(hidden, 7);
        assert_eq!(lines[0], many[0]);
        assert_eq!(lines[PREVIEW_LIMIT - 1], many[PREVIEW_LIMIT - 1]);
    }

    #[test]
    fn preview_split_empty() {
        let (lines, hidden) = preview_split(&[]);
        assert!(lines.is_empty());
        assert_eq!(hidden, 0);
    }
}
