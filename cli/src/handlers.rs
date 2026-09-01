use crate::args::{self, TokenCommands};
use crate::client::{self, ApiClient, ApiResponse, WaitTarget};
use crate::display;
use common::{
    ArtifactConfig, AutorestartPolicy, BatchAction, BatchProgramRequest, BatchProgramResponse,
    CreateProgramRequest, CronCatchup, CronOverlap, ProgramInfo, ProgramLogsResponse,
    ProgramSummary, ResourceLimits, StackApplyRequest, UpdateProgramRequest,
};
use common::{CreateTokenRequest, CreateTokenResponse, UserRole};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

pub struct Context {
    pub client: ApiClient,
    pub base_url: String,
    pub socket_path: Option<PathBuf>,
    pub auth_token: Option<String>,
}

fn api_error_from_body(status: reqwest::StatusCode, body: &str) -> anyhow::Error {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body)
        && let Some(msg) = v.get("message").and_then(|m| m.as_str())
    {
        return anyhow::anyhow!("{msg}");
    }
    anyhow::anyhow!("request failed (HTTP {}): {}", status.as_u16(), body)
}

fn parse_autorestart(s: &str) -> anyhow::Result<AutorestartPolicy> {
    match s.to_lowercase().as_str() {
        "unexpected" => Ok(AutorestartPolicy::Unexpected),
        "true" => Ok(AutorestartPolicy::True),
        "false" => Ok(AutorestartPolicy::False),
        _ => Err(anyhow::anyhow!(
            "autorestart must be unexpected, true, or false"
        )),
    }
}

fn parse_cron_overlap(s: &str) -> anyhow::Result<CronOverlap> {
    match s.to_lowercase().as_str() {
        "skip" => Ok(CronOverlap::Skip),
        "queue" => Ok(CronOverlap::Queue),
        "kill" => Ok(CronOverlap::Kill),
        _ => Err(anyhow::anyhow!("on-overlap must be skip, queue, or kill")),
    }
}

fn parse_cron_catchup(s: &str) -> anyhow::Result<CronCatchup> {
    match s.to_lowercase().as_str() {
        "skip" => Ok(CronCatchup::Skip),
        "latest" => Ok(CronCatchup::Latest),
        "all" => Ok(CronCatchup::All),
        _ => Err(anyhow::anyhow!("catchup must be skip, latest, or all")),
    }
}

fn update_requires_restart(payload: &UpdateProgramRequest, ota_triggered: bool) -> bool {
    if ota_triggered {
        return true;
    }
    payload.name.is_some()
        || payload.command.is_some()
        || payload.args.is_some()
        || payload.env.is_some()
        || payload.env_file.is_some()
        || payload.cwd.is_some()
        || payload.user.is_some()
        || payload.autostart.is_some()
        || payload.retry_limit.is_some()
        || payload.autorestart.is_some()
        || payload.exitcodes.is_some()
        || payload.startsecs.is_some()
        || payload.stopsecs.is_some()
        || payload.priority.is_some()
        || payload.stdout_logfile.is_some()
        || payload.stderr_logfile.is_some()
        || payload.group.is_some()
        || payload.depends_on.is_some()
        || payload.health_check.is_some()
        || payload.hooks.is_some()
        || payload.cron.is_some()
}

fn is_live_resource_limits_update(payload: &UpdateProgramRequest, ota_triggered: bool) -> bool {
    payload.resource_limits.is_some() && !update_requires_restart(payload, ota_triggered)
}

pub async fn check_resp(resp: ApiResponse) -> anyhow::Result<()> {
    let status = resp.status();
    if status.is_success() {
        println!("Success");
    } else {
        let text = resp.text().await.unwrap_or_default();
        if text.trim().is_empty() {
            match status {
                reqwest::StatusCode::FORBIDDEN => eprintln!("Error: Action Forbidden (403)."),
                reqwest::StatusCode::NOT_FOUND => eprintln!("Error: Resource Not Found (404)."),
                _ => eprintln!("Error: Failed with status: {}", status),
            }
        } else {
            if let Ok(err_json) = serde_json::from_str::<serde_json::Value>(&text)
                && let Some(msg) = err_json.get("message").and_then(|m| m.as_str())
            {
                eprintln!("Error: {}", msg);
                return Ok(());
            }
            eprintln!("Error: {}", text);
        }
    }
    Ok(())
}

/// Extra safety knobs shared by every batch command.
#[derive(Clone, Copy, Default)]
pub struct BatchOptions {
    /// Show which programs a batch operation would affect, then exit.
    pub dry_run: bool,
    /// Skip the interactive confirmation prompt (equivalent to answering `y`).
    pub assume_yes: bool,
}

// Batch action handler (server-side atomic operations)
pub async fn handle_batch_action(
    ctx: &Context,
    target: String,
    action: BatchAction,
    wait: bool,
    wait_target: Option<WaitTarget>,
    timeout_sec: u64,
    opts: BatchOptions,
) -> anyhow::Result<()> {
    let action_verb = match &action {
        BatchAction::Start => "START",
        BatchAction::Stop { .. } => "STOP",
        BatchAction::Restart => "RESTART",
        BatchAction::Remove => "REMOVE",
        BatchAction::Signal { signal: _ } => "SIGNAL",
    };

    // 1. Build BatchRequest
    let mut req = BatchProgramRequest {
        target_ids: None,
        group_name: None,
        select_all: false,
        action: action.clone(),
    };

    // 2. Resolve target — keep the full program list so the confirmation
    //    preview and `--dry-run` can show exactly which programs are affected.
    //    For `all`/`@group`, resolve failure is tolerated in the live path
    //    (count is only a hint for the prompt; the batch request itself is the
    //    authority), but `--dry-run` must fail loudly instead of printing a
    //    misleading "0 program(s)" preview.
    let mut resolve_err: Option<anyhow::Error> = None;
    let (count_hint, preview_names) = if target == "all" {
        req.select_all = true;
        match client::resolve_target_details(&ctx.client, &ctx.base_url, &target).await {
            Ok(progs) => {
                let names: Vec<String> = progs.iter().map(|p| p.name.clone()).collect();
                (progs.len(), names)
            }
            Err(e) => {
                resolve_err = Some(e);
                (0, Vec::new())
            }
        }
    } else if let Some(group) = target.strip_prefix('@') {
        req.group_name = Some(group.to_string());
        match client::resolve_target_details(&ctx.client, &ctx.base_url, &target).await {
            Ok(progs) => {
                let names: Vec<String> = progs
                    .iter()
                    .map(|p| {
                        let group_tag = p.group.as_deref().unwrap_or("");
                        if group_tag.is_empty() {
                            p.name.clone()
                        } else {
                            format!("{} (@{})", p.name, group_tag)
                        }
                    })
                    .collect();
                (progs.len(), names)
            }
            Err(e) => {
                resolve_err = Some(e);
                (0, Vec::new())
            }
        }
    } else {
        let progs = client::resolve_target_details(&ctx.client, &ctx.base_url, &target).await?;
        let names: Vec<String> = progs.iter().map(|p| p.name.clone()).collect();
        req.target_ids = Some(progs.into_iter().map(|p| p.id).collect());
        (req.target_ids.as_ref().map_or(0, Vec::len), names)
    };

    // 3. Dry-run: the preview must be authoritative. Resolve errors (daemon
    //    unreachable, unknown group, no programs) are surfaced instead of
    //    pretending nothing would be affected.
    if opts.dry_run {
        if let Some(e) = resolve_err {
            return Err(e);
        }
        display::print_dry_run(action_verb, &target, &preview_names);
        return Ok(());
    }

    // 4. Safety confirmation: skipped when `--yes` or for single targets.
    if !opts.assume_yes && !display::confirm_batch(count_hint, action_verb, &preview_names) {
        println!("Aborted.");
        return Ok(());
    }

    println!(
        "Sending batch request: {} target='{}'...",
        action_verb, target
    );

    // 3. Send single request
    let url = format!("{}/api/v1/programs/batch", ctx.base_url);
    let resp = ctx.client.post(&url).json(&req).send().await?;

    if !resp.status().is_success() {
        eprintln!("Error: {}", resp.text().await?);
        return Ok(());
    }

    // 4. Handle response
    let result: BatchProgramResponse = resp.json().await?;

    // Print failures
    for (id, err) in &result.failed {
        eprintln!("Failed: {} - {}", id, err);
    }

    println!(
        "Success: {}, Failed: {}",
        result.affected.len(),
        result.failed.len()
    );

    // 5. Wait for status change
    if wait && !result.affected.is_empty() {
        // Remove does not need wait
        if matches!(req.action, BatchAction::Remove) {
            return Ok(());
        }

        println!(
            "Waiting for {} programs to reach target state...",
            result.affected.len()
        );

        for id in result.affected {
            // Batch restart cannot know old PIDs; fall back to waiting for UP
            let specific_wait_target = match wait_target {
                Some(WaitTarget::Restarted(_)) => Some(WaitTarget::Up),
                t => t,
            };

            if let Some(target_state) = specific_wait_target
                && let Err(e) = client::wait_for_status(
                    &ctx.client,
                    &ctx.base_url,
                    id,
                    target_state,
                    timeout_sec,
                )
                .await
            {
                eprintln!("   Verification warning for {}: {}", id, e);
            }
        }
    }

    Ok(())
}

pub async fn handle_list(ctx: &Context) -> anyhow::Result<()> {
    let url = format!("{}/api/v1/programs", ctx.base_url);
    let resp = ctx.client.get(&url).send().await?;

    if resp.status().is_success() {
        let programs: Vec<ProgramSummary> = resp.json().await?;
        display::print_list_table(programs);
    } else {
        eprintln!("Error: Failed to fetch list: {}", resp.status());
    }
    Ok(())
}

pub async fn handle_info(ctx: &Context, target: &str) -> anyhow::Result<()> {
    let ids = client::resolve_targets(&ctx.client, &ctx.base_url, target).await?;
    if ids.len() != 1 {
        eprintln!("Error: Info command only supports a single target.");
        return Ok(());
    }

    let url = format!("{}/api/v1/programs/{}", ctx.base_url, ids[0]);
    let resp = ctx.client.get(&url).send().await?;

    if resp.status().is_success() {
        let info: ProgramInfo = resp.json().await?;
        display::print_info(info);
    } else {
        eprintln!("Error: Failed to fetch info: {}", resp.status());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn handle_events(
    ctx: &Context,
    target: &str,
    limit: Option<usize>,
    from: Option<u64>,
    to: Option<u64>,
    event_type: Option<String>,
    exit_code: Option<i32>,
    q: Option<String>,
    stats: bool,
) -> anyhow::Result<()> {
    let ids = client::resolve_targets(&ctx.client, &ctx.base_url, target).await?;
    if ids.len() != 1 {
        eprintln!("Error: Events command only supports a single target.");
        return Ok(());
    }
    let pid = ids[0];

    if stats {
        let url = format!("{}/api/v1/events/stats?program_id={}", ctx.base_url, pid);
        let resp = ctx.client.get(&url).send().await?;
        if resp.status().is_success() {
            let stats: common::EventStats = resp.json().await?;
            display::print_event_stats(&stats);
        } else {
            eprintln!("Error: Failed to fetch event stats: {}", resp.status());
        }
        return Ok(());
    }

    let mut url = format!("{}/api/v1/programs/{}/events", ctx.base_url, pid);
    let mut first = true;
    for (key, val) in [
        ("from", from.map(|v| v.to_string())),
        ("to", to.map(|v| v.to_string())),
        ("event_type", event_type),
        ("exit_code", exit_code.map(|v| v.to_string())),
        ("q", q),
    ] {
        if let Some(v) = val {
            url.push_str(&format!("{}{}={}", if first { "?" } else { "&" }, key, v));
            first = false;
        }
    }

    let resp = ctx.client.get(&url).send().await?;

    if resp.status().is_success() {
        let events: Vec<common::ProgramEventRecord> = resp.json().await?;
        display::print_events(&events, limit);
    } else {
        eprintln!("Error: Failed to fetch events: {}", resp.status());
    }
    Ok(())
}

pub async fn handle_logs(
    ctx: &Context,
    target: &str,
    tail: Option<u32>,
    source: Option<&str>,
    follow: bool,
) -> anyhow::Result<()> {
    let ids = client::resolve_targets(&ctx.client, &ctx.base_url, target).await?;
    if ids.len() != 1 {
        eprintln!("Error: Logs command only supports a single target.");
        return Ok(());
    }
    let id = ids[0];

    if let Some(n) = tail {
        let mut url = format!("{}/api/v1/programs/{}/logs?tail={}", ctx.base_url, id, n);
        if let Some(s) = source {
            url.push_str(&format!("&source={}", s));
        }
        let resp = ctx.client.get(&url).send().await?;
        if resp.status().is_success() {
            let body: ProgramLogsResponse = resp.json().await?;
            for file in &body.logs {
                if body.logs.len() > 1 {
                    eprintln!("--- {} ---", file.source);
                }
                print!("{}", file.content);
                if !file.content.is_empty() && !file.content.ends_with('\n') {
                    println!();
                }
            }
            if body.logs.is_empty() {
                println!("(no log files yet)");
            }
        } else {
            eprintln!("Error: {}", resp.text().await?);
            return Ok(());
        }
        if !follow {
            return Ok(());
        }
    }

    client::monitor_logs(
        &ctx.base_url,
        ctx.socket_path.as_deref(),
        id,
        &ctx.auth_token
            .as_ref()
            .map(|t| format!("&token={t}"))
            .unwrap_or_default(),
    )
    .await?;
    Ok(())
}

pub async fn handle_add(ctx: &Context, cmd: &args::Commands) -> anyhow::Result<()> {
    if let args::Commands::Add {
        name,
        command,
        args,
        env,
        env_file,
        cwd,
        user,
        group,
        autostart,
        numprocs,
        process_name,
        cron,
        on_overlap,
        catchup,
        jitter,
        max_concurrent,
        max_queued,
        cpu,
        memory,
        memory_warn_percent,
        memory_warn_headroom,
        memory_high,
        autorestart,
        exitcodes,
        startsecs,
        stopsecs,
    } = cmd
    {
        let mut env_map = HashMap::new();

        // if let Some(path) = env_file {
        //     if path.exists() {
        //         match dotenvy::from_path_iter(path) {
        //             Ok(iter) => {
        //                 for (k, v) in iter.flatten() {
        //                     env_map.insert(k, v);
        //                 }
        //                 println!("Loaded env file: {:?}", path);
        //             },
        //             Err(e) => return Err(anyhow::anyhow!("Error reading env file: {}", e)),
        //         }
        //     } else {
        //         return Err(anyhow::anyhow!("Env file not found: {:?}", path));
        //     }
        // }

        if let Some(path) = env_file
            && !path.exists()
        {
            println!(
                "Warning: Env file does not exist locally (will be evaluated on server): {:?}",
                path
            );
        }

        for item in env {
            if let Some((k, v)) = item.split_once('=') {
                env_map.insert(k.to_string(), v.to_string());
            }
        }

        let limits = if cpu.is_some()
            || memory.is_some()
            || memory_warn_percent.is_some()
            || memory_warn_headroom.is_some()
            || memory_high.is_some()
        {
            Some(ResourceLimits {
                cpu_quota: *cpu,
                memory_limit: *memory,
                memory_warn_percent: *memory_warn_percent,
                memory_warn_headroom: *memory_warn_headroom,
                memory_high: *memory_high,
            })
        } else {
            None
        };

        #[cfg(not(target_os = "linux"))]
        if limits.is_some() {
            eprintln!(
                "Warning: --cpu/--memory/--memory-* are only enforced on Linux with the isolation plugin loaded."
            );
        }

        let autorestart_policy = match autorestart.as_deref() {
            Some(s) => Some(parse_autorestart(s)?),
            None => None,
        };

        let on_overlap_policy = match on_overlap.as_deref() {
            Some(s) => Some(parse_cron_overlap(s)?),
            None => None,
        };

        let catchup_policy = match catchup.as_deref() {
            Some(s) => Some(parse_cron_catchup(s)?),
            None => None,
        };

        let payload = CreateProgramRequest {
            name: name.clone(),
            command: command.clone(),
            args: args.clone(),
            env: env_map,
            cwd: cwd.clone(),
            user: user.clone(),
            group: group.clone(),
            env_file: env_file.as_ref().map(|p| p.to_string_lossy().to_string()),
            autostart: *autostart,
            numprocs: *numprocs,
            process_name: process_name.clone(),
            retry_limit: 3,
            depends_on: vec![],
            health_check: None,
            hooks: Default::default(),
            artifact: None,
            cron: cron.clone(),
            on_overlap: on_overlap_policy,
            catchup: catchup_policy,
            jitter_sec: *jitter,
            max_concurrent: *max_concurrent,
            max_queued: *max_queued,
            resource_limits: limits,
            autorestart: autorestart_policy.unwrap_or_default(),
            exitcodes: exitcodes.clone().unwrap_or(vec![0]),
            startsecs: startsecs.unwrap_or(10),
            stopsecs: *stopsecs,
            ..Default::default()
        };

        let url = format!("{}/api/v1/programs", ctx.base_url);
        let resp = ctx.client.post(&url).json(&payload).send().await?;
        let status = resp.status();

        if status.is_success() {
            let ids: Vec<Uuid> = resp.json().await?;
            if ids.is_empty() {
                return Err(anyhow::anyhow!(
                    "No program was created. Program name may already exist — run `super list` to verify."
                ));
            }
            if ids.len() == 1 {
                println!("Program created: {}", ids[0]);
            } else {
                println!("Created {} programs:", ids.len());
                for id in ids {
                    println!("  - {}", id);
                }
            }
        } else {
            let body = resp.text().await?;
            return Err(api_error_from_body(status, &body));
        }
    }
    Ok(())
}

pub async fn handle_update(ctx: &Context, cmd: &args::Commands) -> anyhow::Result<()> {
    if let args::Commands::Update {
        target,
        command,
        args,
        cwd,
        user,
        group,
        env,
        env_file,
        autostart,
        retry_limit,
        cron,
        cpu,
        memory,
        memory_warn_percent,
        memory_warn_headroom,
        memory_high,
        no_health_check,
        autorestart,
        exitcodes,
        startsecs,
        stopsecs,
        on_overlap,
        catchup,
        jitter,
        max_concurrent,
        max_queued,
        artifact_url,
        artifact_sha256,
        artifact_destination,
        artifact_extract,
        ..
    } = cmd
    {
        let ids = client::resolve_targets(&ctx.client, &ctx.base_url, target).await?;
        if ids.len() != 1 {
            return Err(anyhow::anyhow!(
                "Update command only supports a single target."
            ));
        }
        let id = ids[0];

        let env_map = if let Some(env_vec) = env {
            let mut map = HashMap::new();
            for item in env_vec {
                if let Some((k, v)) = item.split_once('=') {
                    map.insert(k.to_string(), v.to_string());
                }
            }
            Some(map)
        } else {
            None
        };

        let limits = if cpu.is_some()
            || memory.is_some()
            || memory_warn_percent.is_some()
            || memory_warn_headroom.is_some()
            || memory_high.is_some()
        {
            Some(ResourceLimits {
                cpu_quota: *cpu,
                memory_limit: *memory,
                memory_warn_percent: *memory_warn_percent,
                memory_warn_headroom: *memory_warn_headroom,
                memory_high: *memory_high,
            })
        } else {
            None
        };

        #[cfg(not(target_os = "linux"))]
        if limits.is_some() {
            eprintln!(
                "Warning: --cpu/--memory/--memory-* are only enforced on Linux with the isolation plugin loaded."
            );
        }

        let artifact = if artifact_url.is_some()
            || artifact_sha256.is_some()
            || artifact_destination.is_some()
            || artifact_extract.is_some()
        {
            let source = artifact_url.clone().ok_or_else(|| {
                anyhow::anyhow!("--artifact-url is required when updating artifact")
            })?;
            let checksum = artifact_sha256.clone().ok_or_else(|| {
                anyhow::anyhow!("--artifact-sha256 is required when updating artifact")
            })?;

            let destination = if let Some(dest) = artifact_destination.clone() {
                dest
            } else {
                let url = format!("{}/api/v1/programs/{}", ctx.base_url, id);
                let resp = ctx.client.get(&url).send().await?;
                if !resp.status().is_success() {
                    return Err(anyhow::anyhow!(
                        "Failed to load program: {}",
                        resp.text().await?
                    ));
                }
                let info: ProgramInfo = resp.json().await?;
                info.config
                    .artifact
                    .map(|a| a.destination)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "--artifact-destination is required when the program has no existing artifact config"
                        )
                    })?
            };

            Some(ArtifactConfig {
                source,
                checksum,
                destination,
                extract: artifact_extract.unwrap_or(false),
                restart_policy: "immediate".to_string(),
            })
        } else {
            None
        };

        let ota_triggered = artifact.is_some();

        let mut payload = UpdateProgramRequest {
            name: None,
            command: command.clone(),
            args: args.clone(),
            env: env_map,
            env_file: env_file.clone(),
            cwd: cwd.clone(),
            user: user.clone(),
            group: group.clone(),
            autostart: *autostart,
            retry_limit: *retry_limit,
            autorestart: match autorestart.as_deref() {
                Some(s) => Some(parse_autorestart(s)?),
                None => None,
            },
            exitcodes: exitcodes.clone(),
            startsecs: *startsecs,
            stopsecs: *stopsecs,
            depends_on: None,
            health_check: None,
            hooks: None,
            artifact,
            cron: cron.clone(),
            on_overlap: match on_overlap.as_deref() {
                Some(s) => Some(parse_cron_overlap(s)?),
                None => None,
            },
            catchup: match catchup.as_deref() {
                Some(s) => Some(parse_cron_catchup(s)?),
                None => None,
            },
            jitter_sec: *jitter,
            max_concurrent: *max_concurrent,
            max_queued: *max_queued,
            resource_limits: limits,
            ..Default::default()
        };

        if *no_health_check {
            payload.health_check = Some(common::HealthCheck::Disabled);
        }

        let url = format!("{}/api/v1/programs/{}", ctx.base_url, id);
        let resp = ctx.client.put(&url).json(&payload).send().await?;
        if resp.status().is_success() {
            if ota_triggered {
                println!("OTA update triggered for '{}'.", target);
            } else if is_live_resource_limits_update(&payload, ota_triggered) {
                println!(
                    "Resource limits updated for '{}' (live; no restart required).",
                    target
                );
            } else {
                println!("Configuration updated for '{}'. Restart required.", target);
            }
        } else {
            eprintln!("Error: {}", resp.text().await?);
        }
    }
    Ok(())
}

pub async fn handle_token(ctx: &Context, action: &TokenCommands) -> anyhow::Result<()> {
    let base_url = &ctx.base_url;
    match action {
        TokenCommands::List => {
            let resp = ctx
                .client
                .get(format!("{base_url}/api/v1/auth/tokens"))
                .send()
                .await?;
            if resp.status().is_success() {
                display::print_token_table(resp.json().await?);
            } else {
                eprintln!("Error: Failed with status {}", resp.status());
            }
        }
        TokenCommands::Create { name, role } => {
            let role_enum = match role.to_lowercase().as_str() {
                "admin" => UserRole::Admin,
                "viewer" => UserRole::Viewer,
                _ => UserRole::Operator,
            };
            let req = CreateTokenRequest {
                name: name.clone(),
                role: role_enum,
            };
            let resp = ctx
                .client
                .post(format!("{base_url}/api/v1/auth/tokens"))
                .json(&req)
                .send()
                .await?;

            if resp.status().is_success() {
                let res: CreateTokenResponse = resp.json().await?;
                println!("Token Created: {} ({:?})", res.record.name, res.record.role);
                println!("Token: {}", res.token);
                println!("Save it now! It will not be shown again.");
            } else {
                eprintln!("Error: {}", resp.text().await?);
            }
        }
        TokenCommands::Revoke { id } => {
            let resp = ctx
                .client
                .delete(format!("{base_url}/api/v1/auth/tokens/{id}"))
                .send()
                .await?;
            check_resp(resp).await?;
        }
    }
    Ok(())
}

pub async fn handle_shutdown(ctx: &Context) -> anyhow::Result<()> {
    println!("Initiating System Shutdown...");
    let url = format!("{}/api/v1/system/shutdown", ctx.base_url);
    let resp = ctx.client.post(&url).send().await?;

    if resp.status().is_success() {
        println!("System is shutting down.");
    } else {
        eprintln!("Error: Failed with status {}", resp.status());
    }
    Ok(())
}

pub async fn handle_reload(
    ctx: &Context,
    target: &Option<String>,
    wait: bool,
    timeout: u64,
    opts: BatchOptions,
) -> anyhow::Result<()> {
    if let Some(name) = target {
        handle_batch_action(
            ctx,
            name.clone(),
            BatchAction::Signal {
                signal: "hup".to_string(),
            },
            false,
            None,
            5,
            opts,
        )
        .await?;
        return Ok(());
    }

    println!("Reloading System Configuration...");
    let url = format!(
        "{}/api/v1/system/reload?wait={}&timeout={}",
        ctx.base_url, wait, timeout
    );
    let resp = ctx.client.post(&url).send().await?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        eprintln!("Error: {}", body);
        return Ok(());
    }

    let body: common::ReloadResponse = resp.json().await?;

    if body.affected.is_empty() {
        println!("Configuration reloaded. No programs affected.");
        return Ok(());
    }

    for p in &body.affected {
        let health = match &p.health_error {
            Some(e) if p.status != common::ProcessStatus::Healthy => format!(" ({e})"),
            _ => String::new(),
        };
        println!(
            "  {} ({}) → {}{}",
            p.name,
            p.id,
            format!("{:?}", p.status).to_lowercase(),
            health
        );
    }

    if body.ready {
        if wait {
            println!(
                "Reload complete: all {} affected program(s) healthy after {}s.",
                body.affected.len(),
                body.waited_secs
            );
        } else {
            println!(
                "Configuration reloaded ({} affected program(s)).",
                body.affected.len()
            );
        }
        Ok(())
    } else {
        eprintln!(
            "Reload incomplete: {} program(s) not healthy within {}s.",
            body.affected.len(),
            timeout
        );
        Err(anyhow::anyhow!(
            "Readiness timeout: {} program(s) did not become healthy.",
            body.affected.len()
        ))
    }
}

pub async fn handle_apply(ctx: &Context, file: &std::path::PathBuf) -> anyhow::Result<()> {
    let content = tokio::fs::read_to_string(file).await?;
    let request = common::parse_stack_from_str(&content, file)?;

    println!("Applying stack from {:?}...", file);
    let url = format!("{}/api/v1/stack", ctx.base_url);
    let resp = ctx.client.put(&url).json(&request).send().await?;
    let status = resp.status();

    if status.is_success() {
        let logs: Vec<String> = resp.json().await?;
        for log in logs {
            println!("- {}", log);
        }
        println!("Stack applied successfully.");
    } else {
        let body = resp.text().await?;
        return Err(api_error_from_body(status, &body));
    }
    Ok(())
}

pub async fn handle_export(ctx: &Context, format: args::ExportFormat) -> anyhow::Result<()> {
    let url = format!("{}/api/v1/stack", ctx.base_url);
    let resp = ctx.client.get(&url).send().await?;

    if resp.status().is_success() {
        let json: serde_json::Value = resp.json().await?;
        match format {
            args::ExportFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&json)?);
            }
            args::ExportFormat::Toml => {
                let stack: StackApplyRequest = serde_json::from_value(json)?;
                println!("{}", toml::to_string_pretty(&stack)?);
            }
        }
    } else {
        eprintln!("Error: Failed to export stack: {}", resp.status());
    }
    Ok(())
}
