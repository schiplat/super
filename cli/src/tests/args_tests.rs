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
