use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "super")]
#[command(version, about = "Project Super CLI", long_about = None)]
pub struct Cli {
    /// Specify server address (overrides config file)
    #[arg(short, long)]
    pub server: Option<String>,

    /// API token (or set SUPER_TOKEN env var)
    #[arg(long, env = "SUPER_TOKEN")]
    pub token: Option<String>,

    /// Skip batch confirmation prompts (use for scripts). Equivalent to answering 'y' to every prompt
    #[arg(short = 'y', long, global = true)]
    pub yes: bool,

    /// Show which programs a batch operation would affect and exit without executing
    #[arg(long, global = true)]
    pub dry_run: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Log in and save credentials to ~/.super/cli.json
    Login {
        /// Authentication secret or access token
        secret: String,
        /// Server URL (optional, defaults to configured server)
        #[arg(long)]
        url: Option<String>,
    },
    /// Clear saved credentials from ~/.super/cli.json
    Logout,

    /// Manage API access tokens (requires security plugin)
    Token {
        #[command(subcommand)]
        action: TokenCommands,
    },

    /// Real-time monitoring interface (like htop)
    Top,

    /// List all managed programs
    #[command(alias = "ls")]
    List,

    /// Add a new program to be managed
    Add {
        /// Program name (optional, defaults to command name)
        #[arg(short, long)]
        name: Option<String>,

        /// Enable auto-start (default: true)
        #[arg(long, default_value = "true")]
        autostart: bool,

        /// Working directory
        #[arg(long)]
        cwd: Option<String>,

        /// Environment variables (e.g. -e KEY=VALUE)
        #[arg(short = 'e', long = "env", value_name = "KEY=VALUE")]
        env: Vec<String>,

        /// Load environment variables from a file (.env)
        #[arg(long)]
        env_file: Option<PathBuf>,

        /// Run as specific user (requires root)
        #[arg(long)]
        user: Option<String>,

        /// Group name for organization
        #[arg(long)]
        group: Option<String>,

        /// Number of process instances to start
        #[arg(long, default_value = "1")]
        numprocs: u32,

        /// Process name template (e.g. "worker-{num}")
        #[arg(long)]
        process_name: Option<String>,

        /// Cron expression for scheduled tasks (e.g. "0 0 3 * * *")
        #[arg(long, help_heading = "Resource Isolation")]
        cron: Option<String>,

        /// Cron overlap policy when the previous run is still active: skip (default), queue, kill
        #[arg(long, value_parser = ["skip", "queue", "kill"])]
        on_overlap: Option<String>,

        /// Cron catchup policy for slots missed while the daemon was down: skip (default), latest, all
        #[arg(long, value_parser = ["skip", "latest", "all"])]
        catchup: Option<String>,

        /// Max random delay (seconds) before each cron trigger to spread load
        #[arg(long)]
        jitter: Option<u64>,

        /// Max overlapping cron runs allowed at once (default 1)
        #[arg(long)]
        max_concurrent: Option<u32>,

        /// Cap on queued cron firings when at max_concurrent (default 100; 0 means default)
        #[arg(long)]
        max_queued: Option<u32>,

        /// CPU quota in cores (e.g. 1.5 for 1.5 cores; requires isolation plugin)
        #[arg(long, help_heading = "Resource Isolation")]
        cpu: Option<f32>,

        /// Memory hard limit in MB (e.g. 512; requires isolation plugin)
        #[arg(long, help_heading = "Resource Isolation")]
        memory: Option<u64>,

        /// Memory pressure warning at this % of the hard limit (1–100; 0 disables, default 80)
        #[arg(long, help_heading = "Resource Isolation")]
        memory_warn_percent: Option<u32>,

        /// Memory pressure warning within this many MB of the hard limit (0 disables)
        #[arg(long, help_heading = "Resource Isolation")]
        memory_warn_headroom: Option<u64>,

        /// Kernel soft limit (memory.high) in MB — throttles before the hard limit (0 disables)
        #[arg(long, help_heading = "Resource Isolation")]
        memory_high: Option<u64>,

        /// Auto-restart policy: unexpected (default), true, or false
        #[arg(long, value_parser = ["unexpected", "true", "false"])]
        autorestart: Option<String>,

        /// Comma-separated exit codes considered successful (default: 0)
        #[arg(long, value_delimiter = ',')]
        exitcodes: Option<Vec<i32>>,

        /// Seconds before exit counts as stable start (Supervisor startsecs, default: 10)
        #[arg(long)]
        startsecs: Option<u32>,

        /// Seconds to wait after SIGTERM before SIGKILL (default: server shutdown_timeout)
        #[arg(long)]
        stopsecs: Option<u32>,

        /// Command to execute
        #[arg(required = true)]
        command: String,

        /// Arguments for the command
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Update configuration for an existing program
    Update {
        /// Target program Name or ID
        target: String,

        #[arg(long)]
        command: Option<String>,
        #[arg(long)]
        args: Option<Vec<String>>,
        #[arg(long)]
        cwd: Option<String>,
        #[arg(long)]
        user: Option<String>,
        #[arg(long)]
        group: Option<String>,
        #[arg(short = 'e', long = "env", value_name = "KEY=VALUE")]
        env: Option<Vec<String>>,

        /// Update environment file path (empty string to remove)
        #[arg(long)]
        env_file: Option<String>,

        #[arg(long, value_parser = clap::value_parser!(bool))]
        autostart: Option<bool>,
        #[arg(long)]
        retry_limit: Option<u32>,

        /// Remove health check configuration
        #[arg(long)]
        no_health_check: bool,

        /// Cron expression for scheduled tasks
        #[arg(long, help_heading = "Resource Isolation")]
        cron: Option<String>,

        /// Cron overlap policy when the previous run is still active: skip (default), queue, kill
        #[arg(long, value_parser = ["skip", "queue", "kill"])]
        on_overlap: Option<String>,

        /// Cron catchup policy for slots missed while the daemon was down: skip (default), latest, all
        #[arg(long, value_parser = ["skip", "latest", "all"])]
        catchup: Option<String>,

        /// Max random delay (seconds) before each cron trigger to spread load
        #[arg(long)]
        jitter: Option<u64>,

        /// Max overlapping cron runs allowed at once (default 1; 0 means default)
        #[arg(long)]
        max_concurrent: Option<u32>,

        /// Cap on queued cron firings when at max_concurrent (default 100; 0 means default)
        #[arg(long)]
        max_queued: Option<u32>,

        /// CPU quota in cores (requires isolation plugin)
        #[arg(long, help_heading = "Resource Isolation")]
        cpu: Option<f32>,

        /// Memory hard limit in MB (requires isolation plugin)
        #[arg(long, help_heading = "Resource Isolation")]
        memory: Option<u64>,

        /// Memory pressure warning at this % of the hard limit (1–100; 0 disables, default 80)
        #[arg(long, help_heading = "Resource Isolation")]
        memory_warn_percent: Option<u32>,

        /// Memory pressure warning within this many MB of the hard limit (0 disables)
        #[arg(long, help_heading = "Resource Isolation")]
        memory_warn_headroom: Option<u64>,

        /// Kernel soft limit (memory.high) in MB — throttles before the hard limit (0 disables)
        #[arg(long, help_heading = "Resource Isolation")]
        memory_high: Option<u64>,

        /// Auto-restart policy: unexpected, true, or false
        #[arg(long, value_parser = ["unexpected", "true", "false"])]
        autorestart: Option<String>,

        /// Comma-separated exit codes considered successful
        #[arg(long, value_delimiter = ',')]
        exitcodes: Option<Vec<i32>>,

        /// Seconds before exit counts as stable start
        #[arg(long)]
        startsecs: Option<u32>,

        /// Seconds to wait after SIGTERM before SIGKILL
        #[arg(long)]
        stopsecs: Option<u32>,

        /// OTA download URL (triggers transactional update when checksum changes)
        #[arg(long)]
        artifact_url: Option<String>,

        /// Expected SHA256 hex digest of the OTA artifact
        #[arg(long)]
        artifact_sha256: Option<String>,

        /// Destination path on disk (defaults to existing artifact.destination)
        #[arg(long)]
        artifact_destination: Option<String>,

        /// Extract downloaded archive before swap (default: false)
        #[arg(long, value_parser = clap::value_parser!(bool))]
        artifact_extract: Option<bool>,
    },

    /// Apply a stack configuration file (JSON)
    Apply {
        #[arg(short, long)]
        file: PathBuf,
    },

    // --- Operations ---
    /// Start program(s). Supports `all` or `@group`
    Start {
        target: String,
        /// Wait for the process to reach Running/Healthy state
        #[arg(short, long)]
        wait: bool,

        /// Wait until the process becomes Healthy (readiness check passed)
        #[arg(long, conflicts_with = "wait")]
        wait_healthy: bool,

        /// Timeout in seconds for wait operation (default: 5)
        #[arg(long, default_value = "5")]
        timeout: u64,
    },

    /// Stop program(s). Supports `all` or `@group`
    Stop {
        target: String,
        /// Wait for the process to reach Stopped state
        #[arg(short, long)]
        wait: bool,
        /// Timeout in seconds for wait operation (default: 5)
        #[arg(long, default_value = "5")]
        timeout: u64,

        #[arg(short, long)]
        force: bool,
    },

    /// Restart program(s). Supports `all` or `@group`
    #[command(alias = "rs")]
    Restart {
        target: String,
        /// Wait for the process to reach Running/Healthy state
        #[arg(short, long)]
        wait: bool,
        /// Wait until the process becomes Healthy (readiness check passed)
        #[arg(long, conflicts_with = "wait")]
        wait_healthy: bool,
        /// Timeout in seconds for wait operation (default: 5)
        #[arg(long, default_value = "5")]
        timeout: u64,
    },

    /// Remove program(s). Supports `all` or `@group`
    #[command(alias = "rm")]
    Remove { target: String },

    // --- Monitoring & Signals ---
    /// Show detailed information for a specific program
    Info { target: String },

    /// Show persisted event history for a program (filterable)
    Events {
        target: String,
        /// Number of most recent events to show (default: all)
        #[arg(long)]
        limit: Option<usize>,
        /// Inclusive start of time window (Unix seconds)
        #[arg(long)]
        from: Option<u64>,
        /// Inclusive end of time window (Unix seconds)
        #[arg(long)]
        to: Option<u64>,
        /// Exact event type (e.g. process_fatal, cron_exit)
        #[arg(long)]
        event_type: Option<String>,
        /// Exact exit code
        #[arg(long = "exit-code")]
        exit_code: Option<i32>,
        /// Free-text match on the event message
        #[arg(long)]
        q: Option<String>,
        /// Show retention statistics instead of the event list
        #[arg(long)]
        stats: bool,
    },

    /// Stream or read logs for a specific program
    #[command(alias = "log")]
    Logs {
        target: String,
        /// Read last N lines from disk (omit to stream live logs only)
        #[arg(long)]
        tail: Option<u32>,
        /// Log stream: stdout or stderr (default: both)
        #[arg(long)]
        source: Option<String>,
        /// After --tail, continue streaming live logs via WebSocket
        #[arg(short = 'f', long)]
        follow: bool,
    },

    /// Shutdown the Superd server
    Shutdown,

    /// Export current configuration as a stack file
    Export {
        /// Output format: `toml` (default) or `json`
        #[arg(long, value_enum, default_value_t = ExportFormat::Toml)]
        format: ExportFormat,
    },

    /// Reload configuration or send signals to programs
    Reload {
        /// Target program (supports `all`, `@group`). If empty, reloads system config.
        #[arg(value_name = "TARGET")]
        target: Option<String>,

        /// Wait until all affected programs become Healthy (readiness-aware reload)
        #[arg(long)]
        wait: bool,

        /// Readiness wait timeout in seconds (default: 30)
        #[arg(long, default_value = "30")]
        timeout: u64,
    },

    /// Send a specific signal to program(s)
    Signal {
        target: String,
        /// Signal type: hup, int, term, kill, quit, usr1, usr2
        #[arg(long, default_value = "hup")]
        sig: String,
    },

    /// Validate configuration file without starting the server
    Check {
        /// Path to config file (default: ./conf/super.toml or /etc/super/super.toml)
        #[arg(short, long)]
        file: Option<PathBuf>,
    },

    /// Diagnose local setup and a running daemon (config, connectivity, license)
    Doctor,

    /// List license verifying key ids embedded in this super binary (compile-time keyring)
    Keyring {
        /// Output JSON (for scripts / monitoring)
        #[arg(long)]
        json: bool,
    },
}

/// Output format for `super export`
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum ExportFormat {
    /// TOML — the default stack format
    #[default]
    Toml,
    /// Legacy JSON shape (tooling compatibility)
    Json,
}

#[derive(Subcommand)]
pub enum TokenCommands {
    /// List all active tokens
    List,
    /// Create a new access token
    Create {
        /// Token name/description
        name: String,
        /// Role: viewer, operator, admin
        #[arg(short, long, default_value = "operator")]
        role: String,
    },
    /// Revoke (delete) a token by ID
    #[command(alias = "rm")]
    Revoke { id: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_parses_concurrency_flags() {
        let cli = Cli::try_parse_from([
            "super",
            "add",
            "--name",
            "cron-job",
            "--max-concurrent",
            "4",
            "--max-queued",
            "250",
            "echo",
            "hi",
        ])
        .expect("add must parse");
        match cli.command {
            Commands::Add {
                max_concurrent,
                max_queued,
                ..
            } => {
                assert_eq!(max_concurrent, Some(4));
                assert_eq!(max_queued, Some(250));
            }
            _ => panic!("expected add command, got a different subcommand"),
        }
    }

    #[test]
    fn add_omitted_concurrency_flags_default_none() {
        let cli = Cli::try_parse_from(["super", "add", "echo", "hi"]).expect("add must parse");
        match cli.command {
            Commands::Add {
                max_concurrent,
                max_queued,
                ..
            } => {
                assert_eq!(max_concurrent, None);
                assert_eq!(max_queued, None);
            }
            _ => panic!("expected add command, got a different subcommand"),
        }
    }

    #[test]
    fn update_parses_concurrency_flags() {
        let cli = Cli::try_parse_from([
            "super",
            "update",
            "my-job",
            "--max-concurrent",
            "3",
            "--max-queued",
            "0",
        ])
        .expect("update must parse");
        match cli.command {
            Commands::Update {
                max_concurrent,
                max_queued,
                ..
            } => {
                assert_eq!(max_concurrent, Some(3));
                assert_eq!(max_queued, Some(0));
            }
            _ => panic!("expected update command, got a different subcommand"),
        }
    }

    #[test]
    fn add_rejects_non_numeric_concurrency_flags() {
        let err = Cli::try_parse_from([
            "super",
            "add",
            "--max-concurrent",
            "many",
            "--max-queued",
            "50",
            "echo",
        ]);
        assert!(err.is_err(), "non-numeric max_concurrent must be rejected");
    }

    #[test]
    fn subcommand_aliases_parse() {
        for (alias, kind) in [
            ("ls", "list"),
            ("log", "logs"),
            ("rs", "restart"),
            ("rm", "remove"),
        ] {
            let args: Vec<&str> = if kind == "list" {
                vec!["super", alias]
            } else {
                vec!["super", alias, "myapp"]
            };
            let cli = Cli::try_parse_from(&args)
                .unwrap_or_else(|e| panic!("alias {alias:?} must parse: {e}"));
            match cli.command {
                Commands::List => assert_eq!(kind, "list"),
                Commands::Logs { .. } => assert_eq!(kind, "logs"),
                Commands::Restart { .. } => assert_eq!(kind, "restart"),
                Commands::Remove { .. } => assert_eq!(kind, "remove"),
                _ => panic!("alias {alias:?} resolved to an unexpected command"),
            }
        }
    }
}
