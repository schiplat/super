//! `super auth sso …` — read and change the SSO gate over the API.
//!
//! The CLI never touches `data/auth.db` (it does not link SQLite). Everything
//! here goes through the admin API, so it needs an Admin credential; when SSO
//! is already the gate, use `super login --sso` first.

use std::io::{IsTerminal, Read};

use anyhow::{Context as _, bail};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::args::{AuthCommands, SsoCommands};
use crate::client::ApiResponse;
use crate::handlers::Context;

pub async fn run(ctx: &Context, action: &AuthCommands) -> anyhow::Result<()> {
    match action {
        AuthCommands::Sso { action } => match action {
            SsoCommands::Status => status(ctx).await,
            SsoCommands::Enable { .. } => enable(ctx, action).await,
            SsoCommands::Secret => secret(ctx).await,
            SsoCommands::Test => test(ctx).await,
            SsoCommands::Disable => disable(ctx).await,
        },
    }
}

fn url(ctx: &Context, path: &str) -> String {
    format!("{}{}", ctx.base_url.trim_end_matches('/'), path)
}

fn describe(status: reqwest::StatusCode, body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(body) {
        if let Some(errors) = value.get("errors").and_then(|e| e.as_array()) {
            let joined = errors
                .iter()
                .filter_map(|e| e.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            if !joined.is_empty() {
                return joined;
            }
        }
        for key in ["message", "error", "status"] {
            if let Some(text) = value.get(key).and_then(|v| v.as_str())
                && !text.is_empty()
            {
                return text.to_string();
            }
        }
    }
    let body = body.trim();
    if body.is_empty() {
        format!("HTTP {status}")
    } else {
        format!(
            "HTTP {status}: {}",
            body.chars().take(200).collect::<String>()
        )
    }
}

async fn expect_json(response: ApiResponse, what: &str) -> anyhow::Result<Value> {
    let status = response.status();
    let body = response.text().await?;
    if status == reqwest::StatusCode::NOT_FOUND {
        bail!(
            "{what}: SSO is unavailable (the security plugin owns auth, or this build has no SSO)"
        );
    }
    if !status.is_success() {
        bail!("{what}: {}", describe(status, &body));
    }
    serde_json::from_str(&body).with_context(|| format!("{what}: invalid JSON from the server"))
}

async fn expect_ok(response: ApiResponse, what: &str) -> anyhow::Result<()> {
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let body = response.text().await.unwrap_or_default();
    if status == reqwest::StatusCode::NOT_FOUND {
        bail!(
            "{what}: SSO is unavailable (the security plugin owns auth, or this build has no SSO)"
        );
    }
    bail!("{what}: {}", describe(status, &body))
}

fn format_ts(ts: Option<u64>) -> String {
    ts.and_then(|sec| chrono::DateTime::from_timestamp(sec as i64, 0))
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "-".into())
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(|v| v.as_str()).unwrap_or("-")
}

async fn status(ctx: &Context) -> anyhow::Result<()> {
    let state = expect_json(
        ctx.client
            .get(url(ctx, "/api/v1/auth/status"))
            .send()
            .await?,
        "reading the auth status",
    )
    .await?;
    let view = expect_json(
        ctx.client
            .get(url(ctx, "/api/v1/auth/oidc/config"))
            .send()
            .await?,
        "reading the SSO configuration",
    )
    .await?;

    println!("Gate:          {}", text(&state, "mode"));
    let sso_enabled = state
        .get("sso")
        .and_then(|sso| sso.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    println!(
        "SSO:           {}",
        if sso_enabled { "enabled" } else { "disabled" }
    );

    if view
        .get("configured")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let config = &view["config"];
        println!("Issuer:        {}", text(config, "issuer"));
        println!("Client ID:     {}", text(config, "client_id"));
        println!("Redirect URI:  {}", text(&view, "redirect_uri"));
        println!(
            "Client secret: {}",
            match view.get("secret_updated_at").and_then(Value::as_u64) {
                Some(ts) => format!("set {}", format_ts(Some(ts))),
                None => "not set".into(),
            }
        );
        let rules = config
            .get("role_rules")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        println!("Role rules:    {rules}");
        println!(
            "Updated:       {} by {}",
            format_ts(view.get("updated_at").and_then(Value::as_u64)),
            text(&view, "updated_by")
        );
    } else {
        println!("SSO:           not configured");
    }
    println!(
        "Break-glass:   {}",
        if view
            .get("break_glass")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            "[server].auth_secret is set"
        } else {
            "no auth_secret (a broken SSO store would refuse startup)"
        }
    );
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ConfigView {
    configured: bool,
    config: Option<Value>,
}

fn default_config() -> Value {
    json!({
        "enabled": false,
        "issuer": "",
        "client_id": "",
        "public_base_url": "",
        "scopes": "openid profile email",
        "groups_claim": "groups",
        "token_auth_method": "client_secret_basic",
        "role_rules": [],
        "session_ttl_secs": 12 * 60 * 60,
        "idle_timeout_secs": 60 * 60,
        "allow_insecure_issuer": false,
    })
}

fn parse_role_rule(raw: &str) -> anyhow::Result<Value> {
    let (selector, role) = raw
        .rsplit_once(':')
        .with_context(|| format!("role rule {raw:?}: expected selector=value:role"))?;
    if !matches!(role, "viewer" | "operator" | "admin") {
        bail!("role rule {raw:?}: role must be viewer, operator, or admin");
    }
    let (field, value) = selector
        .split_once('=')
        .with_context(|| format!("role rule {raw:?}: expected selector=value:role"))?;
    let match_object = match field {
        "sub" if !value.is_empty() => json!({ "sub": value }),
        "email" if value.contains('@') => json!({ "email": value }),
        "groups" => {
            let groups: Vec<&str> = value
                .split(',')
                .map(str::trim)
                .filter(|group| !group.is_empty())
                .collect();
            if groups.is_empty() {
                bail!("role rule {raw:?}: groups selector needs at least one group");
            }
            json!({ "groups_any": groups })
        }
        _ => bail!("role rule {raw:?}: selector must be sub=, email=, or groups="),
    };
    Ok(json!({ "match": match_object, "role": role }))
}

fn read_secret_from_stdin() -> anyhow::Result<String> {
    if std::io::stdin().is_terminal() {
        eprintln!("Reading the client secret from the terminal — input is echoed.");
        eprintln!("Safer: read -s SECRET && printf %s \"$SECRET\" | super auth sso secret");
    }
    let mut buffer = String::new();
    std::io::stdin().read_to_string(&mut buffer)?;
    let secret = buffer.trim().to_string();
    if secret.is_empty() {
        bail!("no client secret on stdin");
    }
    Ok(secret)
}

async fn enable(ctx: &Context, action: &SsoCommands) -> anyhow::Result<()> {
    let SsoCommands::Enable {
        issuer,
        client_id,
        base_url,
        secret_stdin,
        allow_insecure,
        disabled,
        role_rules,
    } = action
    else {
        unreachable!("run() only routes Enable here");
    };

    if *secret_stdin {
        let secret = read_secret_from_stdin()?;
        expect_ok(
            ctx.client
                .post(url(ctx, "/api/v1/auth/oidc/secret"))
                .json(&json!({ "secret": secret }))
                .send()
                .await?,
            "storing the client secret",
        )
        .await?;
        println!("Client secret stored.");
    }

    let view: ConfigView = serde_json::from_value(
        expect_json(
            ctx.client
                .get(url(ctx, "/api/v1/auth/oidc/config"))
                .send()
                .await?,
            "reading the SSO configuration",
        )
        .await?,
    )
    .context("unexpected config payload")?;

    let mut config = match (view.configured, view.config) {
        (true, Some(config)) => config,
        _ => default_config(),
    };
    if let Some(value) = issuer {
        config["issuer"] = json!(value);
    }
    if let Some(value) = client_id {
        config["client_id"] = json!(value);
    }
    if let Some(value) = base_url {
        config["public_base_url"] = json!(value);
    }
    if *allow_insecure {
        config["allow_insecure_issuer"] = json!(true);
    }
    if !role_rules.is_empty() {
        let rules = role_rules
            .iter()
            .map(|rule| parse_role_rule(rule))
            .collect::<anyhow::Result<Vec<_>>>()?;
        config["role_rules"] = json!(rules);
    }
    config["enabled"] = json!(!disabled);

    for (field, flag) in [
        ("issuer", "--issuer"),
        ("client_id", "--client-id"),
        ("public_base_url", "--base-url"),
    ] {
        if config
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or("")
            .is_empty()
        {
            bail!("{flag} is required for a first-time setup");
        }
    }
    if config
        .get("role_rules")
        .and_then(Value::as_array)
        .map(Vec::is_empty)
        .unwrap_or(true)
        && !disabled
    {
        bail!("enabling SSO needs at least one --role-rule; otherwise nobody can sign in");
    }

    let saved = expect_json(
        ctx.client
            .put(url(ctx, "/api/v1/auth/oidc/config"))
            .json(&config)
            .send()
            .await?,
        "saving the SSO configuration",
    )
    .await?;
    let gate = saved
        .get("gate")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    println!("Configuration saved. Gate: {gate}.");
    if gate == "sso" {
        println!(
            "This credential is now dormant — run `super login --sso` to get a session for the CLI."
        );
    }
    Ok(())
}

async fn secret(ctx: &Context) -> anyhow::Result<()> {
    let secret = read_secret_from_stdin()?;
    expect_ok(
        ctx.client
            .post(url(ctx, "/api/v1/auth/oidc/secret"))
            .json(&json!({ "secret": secret }))
            .send()
            .await?,
        "storing the client secret",
    )
    .await?;
    println!("Client secret stored. The previous value stays valid for 15 minutes.");
    Ok(())
}

async fn test(ctx: &Context) -> anyhow::Result<()> {
    let discovery = expect_json(
        ctx.client
            .post(url(ctx, "/api/v1/auth/oidc/test"))
            .send()
            .await?,
        "discovery check",
    )
    .await?;
    println!("Discovery OK.");
    for key in [
        "issuer",
        "authorization_endpoint",
        "token_endpoint",
        "jwks_uri",
    ] {
        println!("  {key}: {}", text(&discovery, key));
    }
    Ok(())
}

async fn disable(ctx: &Context) -> anyhow::Result<()> {
    let result = expect_json(
        ctx.client
            .post(url(ctx, "/api/v1/auth/oidc/disable"))
            .send()
            .await?,
        "disabling SSO",
    )
    .await?;
    println!(
        "SSO disabled. {} session(s) revoked.",
        result
            .get("revoked_sessions")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    Ok(())
}
