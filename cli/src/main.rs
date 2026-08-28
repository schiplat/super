mod args;
mod check;
mod client;
mod config;
mod display;
mod doctor;
mod handlers;
mod keyring;
mod session;
mod top;

use args::{Cli, Commands};
use clap::Parser;
use client::WaitTarget;
use common::BatchAction;
use config::CliConfig;
use handlers::Context;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    let mut config = CliConfig::load();

    // Handle Check command (no HTTP client required)
    if let Commands::Check { file } = &args.command {
        return check::run(file.clone());
    }

    if let Commands::Keyring { json } = &args.command {
        return keyring::run(*json);
    }

    if let Some(s) = args.server {
        config.server_url = s;
    }

    match &args.command {
        Commands::Login { secret, url } => {
            return session::login(&mut config, secret, url.as_deref()).await;
        }
        Commands::Logout => {
            return session::logout(&mut config).await;
        }
        _ => {}
    }

    let base_url = config.server_url.trim_end_matches('/').to_string();

    let auth_token = args.token.or(config.auth_token);

    // Doctor aggregates config + daemon + license diagnostics; it tolerates an
    // unreachable daemon, so it runs before any command that would hard-fail.
    if let Commands::Doctor = &args.command {
        return doctor::run(&base_url, auth_token.as_ref()).await;
    }

    let client = client::build_client(auth_token.as_ref())?;
    let ctx = Context {
        client,
        base_url: base_url.clone(),
        auth_token,
    };

    match &args.command {
        Commands::List => handlers::handle_list(&ctx).await?,
        Commands::Add { .. } => handlers::handle_add(&ctx, &args.command).await?,
        Commands::Update { .. } => handlers::handle_update(&ctx, &args.command).await?,

        Commands::Start {
            target,
            wait,
            timeout,
        } => {
            handlers::handle_batch_action(
                &ctx,
                target.clone(),
                BatchAction::Start,
                *wait,
                Some(WaitTarget::Up),
                *timeout,
            )
            .await?
        }

        Commands::Stop {
            target,
            wait,
            timeout,
            force,
        } => {
            handlers::handle_batch_action(
                &ctx,
                target.clone(),
                BatchAction::Stop { force: *force },
                *wait,
                Some(WaitTarget::Down),
                *timeout,
            )
            .await?
        }

        Commands::Restart {
            target,
            wait,
            timeout,
        } => {
            handlers::handle_batch_action(
                &ctx,
                target.clone(),
                BatchAction::Restart,
                *wait,
                Some(WaitTarget::Restarted(None)),
                *timeout,
            )
            .await?
        }

        Commands::Remove { target } => {
            handlers::handle_batch_action(&ctx, target.clone(), BatchAction::Remove, false, None, 5)
                .await?
        }

        Commands::Signal { target, sig } => {
            handlers::handle_batch_action(
                &ctx,
                target.clone(),
                BatchAction::Signal {
                    signal: sig.clone(),
                },
                false,
                None,
                5,
            )
            .await?;
        }

        Commands::Reload { target } => handlers::handle_reload(&ctx, target).await?,
        Commands::Token { action } => handlers::handle_token(&ctx, action).await?,
        Commands::Apply { file } => handlers::handle_apply(&ctx, file).await?,
        Commands::Export => handlers::handle_export(&ctx).await?,
        Commands::Shutdown => handlers::handle_shutdown(&ctx).await?,
        Commands::Info { target } => handlers::handle_info(&ctx, target).await?,
        Commands::Logs {
            target,
            tail,
            source,
            follow,
        } => handlers::handle_logs(&ctx, target, *tail, source.as_deref(), *follow).await?,

        Commands::Top => top::run(&ctx).await?,

        _ => {}
    }

    Ok(())
}
