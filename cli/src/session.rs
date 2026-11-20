use crate::client;
use crate::config::CliConfig;
use serde::Deserialize;
use std::time::Duration;

pub async fn login(config: &mut CliConfig, secret: &str, url: Option<&str>) -> anyhow::Result<()> {
    let target_url = url
        .unwrap_or(&config.server_url)
        .trim_end_matches('/')
        .to_string();

    client::verify_credentials(&target_url, secret).await?;

    config.server_url = target_url;
    config.auth_token = Some(secret.to_string());
    config.save()?;

    println!("Login successful. Credentials saved to ~/.super/cli.json");
    Ok(())
}

#[derive(Debug, Deserialize)]
struct CliStartResponse {
    device_code: String,
    user_code: String,
    verification_url: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Debug, Deserialize)]
struct CliPollResponse {
    status: String,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    user: Option<CliUser>,
}

#[derive(Debug, Deserialize)]
struct CliUser {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    email: Option<String>,
    role: String,
}

fn sso_error_text(status: reqwest::StatusCode, body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            for key in ["message", "status", "error"] {
                if let Some(text) = value.get(key).and_then(|v| v.as_str())
                    && !text.is_empty()
                {
                    return Some(text.to_string());
                }
            }
            None
        })
        .unwrap_or_else(|| format!("HTTP {status}"))
}

/// Device-code SSO login: the CLI shows a URL + code, the user approves it in
/// a signed-in browser, and the poll returns a normal `su_` session token.
pub async fn login_sso(config: &mut CliConfig, url: Option<&str>) -> anyhow::Result<()> {
    let target_url = url
        .unwrap_or(&config.server_url)
        .trim_end_matches('/')
        .to_string();
    let (base, socket) = client::split_server(&target_url);
    let http = client::build_api_client(&target_url, None)?;

    let response = http
        .post(format!("{base}/api/v1/auth/sso/cli/start"))
        .send()
        .await?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        anyhow::bail!(
            "SSO is not enabled on this server (the security plugin owns auth, or this build has no SSO)"
        );
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!(
            "could not start the SSO sign-in: {}",
            sso_error_text(status, &body)
        );
    }
    let start: CliStartResponse = response.json().await?;

    match &socket {
        Some(path) => println!(
            "Approve this sign-in from a browser that can reach the dashboard ({base}, socket {}).",
            path.display()
        ),
        None => println!("Approve this sign-in from a signed-in browser:"),
    }
    println!();
    println!("  {}", start.verification_url);
    println!();
    println!("Confirm code {} to finish.", start.user_code);
    println!();

    let interval = start.interval.max(1);
    let deadline = std::time::Instant::now() + Duration::from_secs(start.expires_in);
    let mut waiting = false;
    loop {
        if std::time::Instant::now() >= deadline {
            anyhow::bail!("the sign-in request expired before it was approved");
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;

        let response = http
            .post(format!("{base}/api/v1/auth/sso/cli/poll"))
            .json(&serde_json::json!({ "device_code": start.device_code }))
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if status == reqwest::StatusCode::NOT_FOUND {
            anyhow::bail!("this sign-in request is unknown or was already used");
        }
        if !status.is_success() {
            anyhow::bail!("sign-in failed: {}", sso_error_text(status, &body));
        }
        let poll: CliPollResponse = serde_json::from_str(&body)?;
        match poll.status.as_str() {
            "pending" => {
                if !waiting {
                    println!("Waiting for approval…");
                    waiting = true;
                }
            }
            "ok" => {
                let Some(token) = poll.token else {
                    anyhow::bail!("the server reported success without a token");
                };
                let who = poll.user.as_ref().map(|user| {
                    let name = user
                        .name
                        .clone()
                        .or_else(|| user.email.clone())
                        .unwrap_or_else(|| "unknown".to_string());
                    format!("{name} ({})", user.role)
                });
                config.server_url = target_url;
                config.auth_token = Some(token);
                config.save()?;
                match who {
                    Some(who) => println!(
                        "Signed in via SSO as {who}. Credentials saved to ~/.super/cli.json"
                    ),
                    None => println!("Signed in via SSO. Credentials saved to ~/.super/cli.json"),
                }
                return Ok(());
            }
            other => anyhow::bail!("unexpected sign-in status {other}"),
        }
    }
}

pub async fn logout(config: &mut CliConfig) -> anyhow::Result<()> {
    if config.auth_token.is_none() {
        println!("Not logged in.");
        return Ok(());
    }

    if let Some(token) = config.auth_token.clone() {
        let base = config.server_url.trim_end_matches('/');
        // Best-effort: end sticky root session when logging out with auth_secret.
        let _ = client::server_logout(base, &token).await;
    }

    config.auth_token = None;
    config.save()?;
    println!("Logged out.");
    Ok(())
}
