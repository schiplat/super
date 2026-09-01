use common::HealthCheck;
use common::security::{FetchUrlPolicy, validate_outbound_url};
use std::process::Stdio;
use std::time::Duration;
use tokio::net::TcpStream;

/// Result of a single health probe, with a human-readable failure reason when unhealthy.
#[derive(Debug, Clone)]
pub struct CheckOutcome {
    pub healthy: bool,
    pub detail: Option<String>,
}

impl CheckOutcome {
    pub fn ok() -> Self {
        Self {
            healthy: true,
            detail: None,
        }
    }

    pub fn fail(detail: impl Into<String>) -> Self {
        Self {
            healthy: false,
            detail: Some(detail.into()),
        }
    }
}

/// Run one health check, honoring the probe's configured `timeout_secs`.
pub async fn perform_check(check: &HealthCheck) -> CheckOutcome {
    match check {
        HealthCheck::Tcp { host, port, .. } => {
            check_tcp(host, *port, Duration::from_secs(check.timeout_secs())).await
        }
        HealthCheck::Http { url, method, .. } => {
            check_http(
                url,
                method.as_deref(),
                Duration::from_secs(check.timeout_secs()),
            )
            .await
        }
        HealthCheck::Exec { command, .. } => {
            check_exec(command, Duration::from_secs(check.timeout_secs())).await
        }
        HealthCheck::Disabled => CheckOutcome::ok(),
    }
}

async fn check_tcp(host: &str, port: u16, timeout: Duration) -> CheckOutcome {
    let addr = format!("{}:{}", host, port);
    match tokio::time::timeout(timeout, TcpStream::connect(&addr)).await {
        Ok(Ok(_)) => CheckOutcome::ok(),
        Ok(Err(e)) => CheckOutcome::fail(format!("TCP {}: connection failed: {}", addr, e)),
        Err(_) => CheckOutcome::fail(format!("TCP {}: connection timed out", addr)),
    }
}

async fn check_http(url: &str, method: Option<&str>, timeout: Duration) -> CheckOutcome {
    if let Err(e) = validate_outbound_url(url, FetchUrlPolicy::HealthCheck) {
        return CheckOutcome::fail(format!("Health check URL rejected: {e}"));
    }

    // Rebuild per probe so the timeout matches the configured `timeout_secs`.
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .user_agent("Superd-HealthCheck/1.0")
        // Health checks hit localhost/LAN; disable proxy to skip extra hops
        .no_proxy()
        .build()
        .expect("Failed to initialize HealthCheck HTTP Client");

    let method_str = method.unwrap_or("GET");
    let method = match method_str.to_uppercase().as_str() {
        "POST" => reqwest::Method::POST,
        "HEAD" => reqwest::Method::HEAD,
        "PUT" => reqwest::Method::PUT,
        _ => reqwest::Method::GET,
    };

    match client.request(method.clone(), url).send().await {
        Ok(resp) if resp.status().is_success() => CheckOutcome::ok(),
        Ok(resp) => CheckOutcome::fail(format!(
            "HTTP {} {} returned {}",
            method_str,
            url,
            resp.status()
        )),
        Err(e) => CheckOutcome::fail(format!("HTTP {} {} failed: {}", method_str, url, e)),
    }
}

async fn check_exec(command: &str, timeout: Duration) -> CheckOutcome {
    let check_future = async {
        match tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .output()
            .await
        {
            Ok(output) if output.status.success() => CheckOutcome::ok(),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let trimmed = stderr.trim();
                if !trimmed.is_empty() {
                    CheckOutcome::fail(format!("exec {:?}: {}", command, trimmed))
                } else {
                    CheckOutcome::fail(format!(
                        "exec {:?} exited with code {:?}",
                        command,
                        output.status.code()
                    ))
                }
            }
            Err(e) => CheckOutcome::fail(format!("exec {:?}: spawn failed: {}", command, e)),
        }
    };

    match tokio::time::timeout(timeout, check_future).await {
        Ok(outcome) => outcome,
        Err(_) => CheckOutcome::fail(format!("exec {:?}: timed out", command)),
    }
}
