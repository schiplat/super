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
use handlers::{BatchOptions, Context};

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

    let explicit_server = args.server.is_some();
    if let Some(s) = args.server {
        config.server_url = s;
    }

    // Auto-discovery: without `--server` and without a persisted choice in
    // `~/.super/cli.json`, prefer the local default Unix socket when present
    // (`$SUPER_ROOT/run/superd.sock`), falling back to the configured TCP URL.
    if !explicit_server
        && !CliConfig::exists()
        && let Some(sock) = client::discover_default_socket()
    {
        config.server_url = format!("unix://{}", sock.display());
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

    let (base_url, socket_path) = client::split_server(&config.server_url);

    let auth_token = args.token.or(config.auth_token);

    // Doctor aggregates config + daemon + license diagnostics; it tolerates an
    // unreachable daemon, so it runs before any command that would hard-fail.
    if let Commands::Doctor = &args.command {
        return doctor::run(&config.server_url, auth_token.as_ref()).await;
    }

    let client = client::build_api_client(&config.server_url, auth_token.as_ref())?;
    let ctx = Context {
        client,
        base_url: base_url.clone(),
        socket_path,
        auth_token,
    };

    // Batch-safety flags apply to every start/stop/restart/remove/signal call.
    let batch_opts = BatchOptions {
        dry_run: args.dry_run,
        assume_yes: args.yes,
    };

    match &args.command {
        Commands::List => handlers::handle_list(&ctx).await?,
        Commands::Add { .. } => handlers::handle_add(&ctx, &args.command).await?,
        Commands::Update { .. } => handlers::handle_update(&ctx, &args.command).await?,

        Commands::Start {
            target,
            wait,
            wait_healthy,
            timeout,
        } => {
            handlers::handle_batch_action(
                &ctx,
                target.clone(),
                BatchAction::Start,
                *wait || *wait_healthy,
                if *wait_healthy {
                    Some(WaitTarget::Healthy)
                } else {
                    Some(WaitTarget::Up)
                },
                *timeout,
                batch_opts,
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
                batch_opts,
            )
            .await?
        }

        Commands::Restart {
            target,
            wait,
            wait_healthy,
            timeout,
        } => {
            handlers::handle_batch_action(
                &ctx,
                target.clone(),
                BatchAction::Restart,
                *wait || *wait_healthy,
                if *wait_healthy {
                    Some(WaitTarget::Healthy)
                } else {
                    Some(WaitTarget::Restarted(None))
                },
                *timeout,
                batch_opts,
            )
            .await?
        }

        Commands::Remove { target } => {
            handlers::handle_batch_action(
                &ctx,
                target.clone(),
                BatchAction::Remove,
                false,
                None,
                5,
                batch_opts,
            )
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
                batch_opts,
            )
            .await?;
        }

        Commands::Reload {
            target,
            wait,
            timeout,
        } => handlers::handle_reload(&ctx, target, *wait, *timeout, batch_opts).await?,
        Commands::Token { action } => handlers::handle_token(&ctx, action).await?,
        Commands::Apply { file } => handlers::handle_apply(&ctx, file).await?,
        Commands::Export { format } => handlers::handle_export(&ctx, *format).await?,
        Commands::Shutdown => handlers::handle_shutdown(&ctx).await?,
        Commands::Info { target } => handlers::handle_info(&ctx, target).await?,
        Commands::Events { target, limit } => handlers::handle_events(&ctx, target, *limit).await?,
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
