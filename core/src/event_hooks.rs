use crate::extension::Extension;
use common::SystemEvent;
use common::config::EventHookConfig;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

/// Notify extensions and run configured OSS event hooks.
pub fn emit(extension: &Arc<dyn Extension>, hooks: &[EventHookConfig], event: SystemEvent) {
    extension.on_event(event.clone());
    dispatch(hooks, &event);
}

pub fn dispatch(hooks: &[EventHookConfig], event: &SystemEvent) {
    let matching: Vec<EventHookConfig> = hooks
        .iter()
        .filter(|h| matches_hook(h, event))
        .cloned()
        .collect();

    if matching.is_empty() {
        return;
    }

    let json_body = match build_payload(event) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Failed to serialize event hook payload: {}", e);
            return;
        }
    };

    let env = build_env(event);

    tokio::spawn(async move {
        for hook in matching {
            let body = json_body.clone();
            let env = env.clone();
            let timeout = hook.timeout_secs;
            let hook_id = hook.id.clone();

            // Webhook mode: POST the event JSON to the configured URL.
            if let Some(url) = hook.url.clone() {
                let headers = hook.headers.clone().unwrap_or_default();
                let fut = async move {
                    post_webhook(&url, &headers, &body, timeout, hook_id.as_deref()).await;
                };
                if hook.r#async {
                    tokio::spawn(fut);
                } else {
                    fut.await;
                }
                continue;
            }

            let cmd = hook.command.clone();

            if hook.r#async {
                tokio::spawn(async move {
                    run_one(&cmd, &body, &env, timeout, hook_id.as_deref()).await;
                });
            } else {
                run_one(&cmd, &body, &env, timeout, hook_id.as_deref()).await;
            }
        }
    });
}

/// POST the event payload to an HTTP(S) webhook.
async fn post_webhook(
    url: &str,
    headers: &HashMap<String, String>,
    json_body: &str,
    timeout_secs: u64,
    id: Option<&str>,
) {
    let label = id.unwrap_or(url);
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
    else {
        tracing::warn!("Event webhook '{}': failed to build HTTP client", label);
        return;
    };

    let mut req = client.post(url).header("Content-Type", "application/json");
    for (k, v) in headers {
        req = req.header(k, v);
    }

    match req.body(json_body.to_string()).send().await {
        Ok(resp) if resp.status().is_success() => {
            tracing::debug!(
                "Event webhook '{}' delivered (HTTP {})",
                label,
                resp.status()
            );
        }
        Ok(resp) => {
            tracing::warn!(
                "Event webhook '{}' rejected (HTTP {})",
                label,
                resp.status()
            );
        }
        Err(e) => {
            tracing::warn!("Event webhook '{}' failed: {}", label, e);
        }
    }
}

async fn run_one(
    command: &str,
    json_body: &str,
    env: &HashMap<String, String>,
    timeout_secs: u64,
    id: Option<&str>,
) {
    let label = id.unwrap_or(command);
    match crate::hooks::run_hook_with_stdin(command, env, Some(json_body), timeout_secs).await {
        Ok(true) => tracing::debug!("Event hook '{}' completed", label),
        Ok(false) => tracing::warn!("Event hook '{}' exited non-zero", label),
        Err(e) => tracing::warn!("Event hook '{}' failed: {}", label, e),
    }
}

fn matches_hook(hook: &EventHookConfig, event: &SystemEvent) -> bool {
    let event_type = event.event_type();
    let event_match = hook.events.iter().any(|e| e == "*" || e == event_type);
    if !event_match {
        return false;
    }

    if hook.programs.iter().any(|p| p == "*") {
        return true;
    }

    match event.program_name() {
        Some(name) => hook.programs.iter().any(|p| p == name),
        None => false,
    }
}

/// Build the JSON webhook/hook payload for a system event.
///
/// Exposed for integration tests; the canonical entry point is [`emit`].
pub fn build_payload(event: &SystemEvent) -> anyhow::Result<String> {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let program_json = match event {
        SystemEvent::ProcessFatal {
            program_id,
            program_name,
            pid,
            uptime_secs,
            ..
        }
        | SystemEvent::ProcessBackoff {
            program_id,
            program_name,
            pid,
            uptime_secs,
            ..
        } => Some(json!({
            "id": program_id,
            "name": program_name,
            "pid": pid,
            "uptime_secs": uptime_secs,
        })),
        SystemEvent::ProcessStarted {
            program_id,
            program_name,
            pid,
        } => Some(json!({
            "id": program_id,
            "name": program_name,
            "pid": pid,
            "uptime_secs": 0,
        })),
        SystemEvent::ProcessRecovered {
            program_id,
            program_name,
            pid,
            uptime_sec,
        } => Some(json!({
            "id": program_id,
            "name": program_name,
            "pid": pid,
            "uptime_secs": uptime_sec,
        })),
        SystemEvent::MemoryPressure {
            program_id,
            program_name,
            pid,
            ..
        }
        | SystemEvent::MemoryOomKill {
            program_id,
            program_name,
            pid,
            ..
        } => Some(json!({
            "id": program_id,
            "name": program_name,
            "pid": pid,
        })),
        SystemEvent::SystemStartup { .. } | SystemEvent::SystemShutdown => None,
    };

    let payload: Value = match event {
        SystemEvent::ProcessFatal {
            exit_code,
            signal,
            msg,
            log_tail,
            ..
        } => json!({
            "exit_code": exit_code,
            "signal": signal,
            "msg": msg,
            "log_tail": log_tail,
        }),
        SystemEvent::ProcessBackoff {
            exit_code,
            signal,
            retry_count,
            ..
        } => json!({
            "exit_code": exit_code,
            "signal": signal,
            "retry_count": retry_count,
        }),
        SystemEvent::ProcessStarted { .. } => json!({}),
        SystemEvent::ProcessRecovered { uptime_sec, .. } => json!({ "uptime_sec": uptime_sec }),
        SystemEvent::SystemStartup { hostname } => json!({ "hostname": hostname }),
        SystemEvent::SystemShutdown => json!({}),
        SystemEvent::MemoryPressure {
            usage_bytes,
            limit_bytes,
            warn_bytes,
            ..
        } => json!({
            "usage_bytes": usage_bytes,
            "limit_bytes": limit_bytes,
            "warn_bytes": warn_bytes,
        }),
        SystemEvent::MemoryOomKill {
            anon_bytes,
            limit_bytes,
            usage_bytes,
            ..
        } => json!({
            "usage_bytes": usage_bytes,
            "limit_bytes": limit_bytes,
            "anon_bytes": anon_bytes,
        }),
    };

    let body = json!({
        "event": event.event_type(),
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "hostname": hostname,
        "version": env!("CARGO_PKG_VERSION"),
        "program": program_json,
        "payload": payload,
    });

    Ok(serde_json::to_string(&body)?)
}

fn build_env(event: &SystemEvent) -> HashMap<String, String> {
    let mut env = HashMap::new();
    env.insert("SUPER_EVENT".to_string(), event.event_type().to_string());

    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    env.insert("SUPER_HOSTNAME".to_string(), hostname);

    match event {
        SystemEvent::ProcessFatal {
            program_id,
            program_name,
            pid,
            uptime_secs,
            exit_code,
            ..
        }
        | SystemEvent::ProcessBackoff {
            program_id,
            program_name,
            pid,
            uptime_secs,
            exit_code,
            ..
        } => {
            env.insert("SUPER_ID".to_string(), program_id.to_string());
            env.insert("SUPER_NAME".to_string(), program_name.clone());
            if let Some(p) = pid {
                env.insert("SUPER_PID".to_string(), p.to_string());
            }
            env.insert("SUPER_UPTIME_SECS".to_string(), uptime_secs.to_string());
            if let Some(c) = exit_code {
                env.insert("SUPER_EXIT_CODE".to_string(), c.to_string());
            }
        }
        SystemEvent::ProcessStarted {
            program_id,
            program_name,
            pid,
        } => {
            env.insert("SUPER_ID".to_string(), program_id.to_string());
            env.insert("SUPER_NAME".to_string(), program_name.clone());
            env.insert("SUPER_PID".to_string(), pid.to_string());
        }
        SystemEvent::ProcessRecovered {
            program_id,
            program_name,
            pid,
            uptime_sec,
        } => {
            env.insert("SUPER_ID".to_string(), program_id.to_string());
            env.insert("SUPER_NAME".to_string(), program_name.clone());
            if let Some(p) = pid {
                env.insert("SUPER_PID".to_string(), p.to_string());
            }
            env.insert("SUPER_UPTIME_SECS".to_string(), uptime_sec.to_string());
        }
        SystemEvent::MemoryPressure {
            program_id,
            program_name,
            pid,
            usage_bytes,
            limit_bytes,
            ..
        }
        | SystemEvent::MemoryOomKill {
            program_id,
            program_name,
            pid,
            usage_bytes,
            limit_bytes,
            ..
        } => {
            env.insert("SUPER_ID".to_string(), program_id.to_string());
            env.insert("SUPER_NAME".to_string(), program_name.clone());
            if let Some(p) = pid {
                env.insert("SUPER_PID".to_string(), p.to_string());
            }
            env.insert("SUPER_USAGE_BYTES".to_string(), usage_bytes.to_string());
            env.insert("SUPER_LIMIT_BYTES".to_string(), limit_bytes.to_string());
            if let SystemEvent::MemoryPressure { warn_bytes, .. } = event {
                env.insert("SUPER_WARN_BYTES".to_string(), warn_bytes.to_string());
            }
        }
        SystemEvent::SystemStartup { .. } | SystemEvent::SystemShutdown => {}
    }

    env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_event_and_program_filters() {
        let hook = EventHookConfig {
            command: "true".into(),
            url: None,
            headers: None,
            events: vec!["process_fatal".into()],
            programs: vec!["web".into()],
            r#async: true,
            timeout_secs: 5,
            id: None,
        };
        let id = uuid::Uuid::new_v4();
        let event = SystemEvent::ProcessFatal {
            program_id: id,
            program_name: "web".into(),
            pid: None,
            uptime_secs: 0,
            exit_code: None,
            signal: None,
            msg: "x".into(),
            log_tail: None,
        };
        assert!(matches_hook(&hook, &event));
        let other = SystemEvent::ProcessFatal {
            program_id: id,
            program_name: "worker".into(),
            pid: None,
            uptime_secs: 0,
            exit_code: None,
            signal: None,
            msg: "x".into(),
            log_tail: None,
        };
        assert!(!matches_hook(&hook, &other));
    }
}
