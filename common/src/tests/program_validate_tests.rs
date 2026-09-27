use super::*;
use std::collections::HashMap;

fn tmp_logs() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn minimal_create(command: &str) -> CreateProgramRequest {
    CreateProgramRequest {
        command: command.into(),
        ..Default::default()
    }
}

#[test]
fn create_rejects_empty_command() {
    let dir = tmp_logs();
    let err = validate_create_program_request(&minimal_create("  "), dir.path()).unwrap_err();
    assert!(err.to_string().contains("command:"), "{err}");
}

#[test]
fn create_accepts_minimal() {
    let dir = tmp_logs();
    validate_create_program_request(&minimal_create("/bin/true"), dir.path()).unwrap();
}

#[test]
fn source_label_roundtrip_and_clear_sentinel() {
    // Missing field deserializes to None (old snapshot / old clients).
    let req: CreateProgramRequest = serde_json::from_str(r#"{"command":"/bin/true"}"#).unwrap();
    assert!(req.source.is_none());
    // Present field round-trips.
    let req: CreateProgramRequest =
        serde_json::from_str(r#"{"command":"/bin/true","source":"import:supervisor"}"#).unwrap();
    assert_eq!(req.source.as_deref(), Some("import:supervisor"));
    // Serde omits the field when unset (clean old-format output).
    let json = serde_json::to_string(&CreateProgramRequest {
        command: "/bin/true".into(),
        source: None,
        ..Default::default()
    })
    .unwrap();
    assert!(!json.contains("\"source\""), "{json}");
}

#[test]
fn source_label_validation() {
    let dir = tmp_logs();
    // Accepted: short, printable, with the kind:detail convention.
    let mut req = minimal_create("/bin/true");
    req.source = Some("import:supervisor".into());
    validate_create_program_request(&req, dir.path()).unwrap();
    // Rejected: control characters / newline.
    req.source = Some("bad\nlabel".into());
    let err = validate_create_program_request(&req, dir.path()).unwrap_err();
    assert!(err.to_string().contains("source:"), "{err}");
    // Rejected: over-long.
    req.source = Some("x".repeat(129));
    let err = validate_create_program_request(&req, dir.path()).unwrap_err();
    assert!(err.to_string().contains("source:"), "{err}");
}

#[test]
fn create_rejects_http_health_without_scheme() {
    let dir = tmp_logs();
    let mut req = minimal_create("/bin/true");
    req.health_check = Some(HealthCheck::Http {
        url: "127.0.0.1/health".into(),
        method: None,
        interval_secs: 0,
        timeout_secs: 0,
        start_period_secs: 0,
        max_failures: 0,
    });
    let err = validate_create_program_request(&req, dir.path()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("health_check.url"), "{msg}");
    assert!(msg.contains("127.0.0.1/health"), "{msg}");
}

#[test]
fn create_rejects_health_tuning_out_of_bounds() {
    let dir = tmp_logs();
    let mut req = minimal_create("/bin/true");
    req.health_check = Some(HealthCheck::Exec {
        command: "true".into(),
        interval_secs: crate::MAX_HEALTH_INTERVAL_SECS + 1,
        timeout_secs: 0,
        start_period_secs: 0,
        max_failures: 0,
    });
    let err = validate_create_program_request(&req, dir.path()).unwrap_err();
    assert!(err.to_string().contains("interval_secs"), "{err}");

    let mut req = minimal_create("/bin/true");
    req.health_check = Some(HealthCheck::Exec {
        command: "true".into(),
        interval_secs: 0,
        timeout_secs: crate::MAX_HEALTH_TIMEOUT_SECS + 1,
        start_period_secs: 0,
        max_failures: 0,
    });
    let err = validate_create_program_request(&req, dir.path()).unwrap_err();
    assert!(err.to_string().contains("timeout_secs"), "{err}");

    let mut req = minimal_create("/bin/true");
    req.health_check = Some(HealthCheck::Exec {
        command: "true".into(),
        interval_secs: 0,
        timeout_secs: 0,
        start_period_secs: crate::MAX_HEALTH_INTERVAL_SECS + 1,
        max_failures: 0,
    });
    let err = validate_create_program_request(&req, dir.path()).unwrap_err();
    assert!(err.to_string().contains("start_period_secs"), "{err}");

    let mut req = minimal_create("/bin/true");
    req.health_check = Some(HealthCheck::Exec {
        command: "true".into(),
        interval_secs: 0,
        timeout_secs: 0,
        start_period_secs: 0,
        max_failures: crate::MAX_HEALTH_MAX_FAILURES + 1,
    });
    let err = validate_create_program_request(&req, dir.path()).unwrap_err();
    assert!(err.to_string().contains("max_failures"), "{err}");
}

#[test]
fn create_accepts_health_tuning_defaults_and_zero() {
    let dir = tmp_logs();
    let mut req = minimal_create("/bin/true");
    req.health_check = Some(HealthCheck::Exec {
        command: "true".into(),
        interval_secs: 0,
        timeout_secs: 0,
        start_period_secs: 0,
        max_failures: 0,
    });
    validate_create_program_request(&req, dir.path()).unwrap();
}

#[test]
fn create_rejects_unknown_json_field() {
    let err =
        serde_json::from_str::<CreateProgramRequest>(r#"{"command":"/bin/true","not_a_field":1}"#)
            .unwrap_err();
    assert!(err.to_string().contains("unknown") || err.to_string().contains("not_a_field"));
    assert!(err.line() >= 1 && err.column() >= 1);
    let located = format_serde_json_error("stack.json", &err);
    assert!(located.starts_with("stack.json:"), "{located}");
    assert!(located.contains("not_a_field") || located.contains("unknown"));
}

#[test]
fn update_rejects_empty_command() {
    let dir = tmp_logs();
    let req = UpdateProgramRequest {
        command: Some("".into()),
        ..Default::default()
    };
    let err = validate_update_program_request(&req, dir.path()).unwrap_err();
    assert!(err.to_string().contains("command:"), "{err}");
}

#[test]
fn create_rejects_bad_artifact_checksum() {
    let dir = tmp_logs();
    let mut req = minimal_create("/bin/true");
    req.artifact = Some(ArtifactConfig {
        source: "https://example.com/a.tar.gz".into(),
        checksum: "abc".into(),
        extract: false,
        destination: "/tmp/x".into(),
        restart_policy: "immediate".into(),
        ..Default::default()
    });
    let err = validate_create_program_request(&req, dir.path()).unwrap_err();
    assert!(err.to_string().contains("artifact.checksum"), "{err}");
}

#[test]
fn create_rejects_bad_restart_policy() {
    let dir = tmp_logs();
    let mut req = minimal_create("/bin/true");
    req.artifact = Some(ArtifactConfig {
        source: "https://example.com/a".into(),
        checksum: "a".repeat(64),
        extract: false,
        destination: "/tmp/x".into(),
        restart_policy: "always".into(),
        ..Default::default()
    });
    let err = validate_create_program_request(&req, dir.path()).unwrap_err();
    assert!(err.to_string().contains("artifact.restart_policy"), "{err}");
}

#[test]
fn parse_restart_policy_variants() {
    use super::{ArtifactRestartPolicy, parse_artifact_restart_policy};
    assert_eq!(
        parse_artifact_restart_policy("").unwrap(),
        ArtifactRestartPolicy::Immediate
    );
    assert_eq!(
        parse_artifact_restart_policy("manual").unwrap(),
        ArtifactRestartPolicy::Manual
    );
    assert_eq!(
        parse_artifact_restart_policy("signal").unwrap(),
        ArtifactRestartPolicy::Signal { signal: "hup" }
    );
    assert_eq!(
        parse_artifact_restart_policy("signal:USR1").unwrap(),
        ArtifactRestartPolicy::Signal { signal: "usr1" }
    );
    assert!(parse_artifact_restart_policy("signal:kill").is_err());
}

#[test]
fn create_rejects_signal_restart_without_health_check() {
    let dir = tmp_logs();
    let mut req = minimal_create("/bin/true");
    req.artifact = Some(ArtifactConfig {
        source: "https://example.com/a".into(),
        checksum: "a".repeat(64),
        extract: false,
        destination: "/tmp/x".into(),
        restart_policy: "signal:hup".into(),
        ..Default::default()
    });
    let err = validate_create_program_request(&req, dir.path()).unwrap_err();
    assert!(
        err.to_string().contains("signal") && err.to_string().contains("health_check"),
        "{err}"
    );
}

#[test]
fn create_allows_signal_restart_with_health_check() {
    let dir = tmp_logs();
    let mut req = minimal_create("/bin/true");
    req.health_check = Some(HealthCheck::Exec {
        command: "true".into(),
        interval_secs: 5,
        timeout_secs: 0,
        start_period_secs: 0,
        max_failures: 0,
    });
    req.artifact = Some(ArtifactConfig {
        source: "https://example.com/a".into(),
        checksum: "a".repeat(64),
        extract: false,
        destination: "/tmp/x".into(),
        restart_policy: "signal".into(),
        ..Default::default()
    });
    validate_create_program_request(&req, dir.path()).unwrap();
}

#[test]
fn signal_restart_missing_health_probe_detects_gap() {
    let art = ArtifactConfig {
        source: "https://example.com/a".into(),
        checksum: "a".repeat(64),
        extract: false,
        destination: "/tmp/x".into(),
        restart_policy: "signal:hup".into(),
        ..Default::default()
    };
    assert!(signal_restart_missing_health_probe(Some(&art), None));
    assert!(signal_restart_missing_health_probe(
        Some(&art),
        Some(&HealthCheck::Disabled)
    ));
    let hc = HealthCheck::Tcp {
        host: "127.0.0.1".into(),
        port: 8080,
        interval_secs: 5,
        timeout_secs: 0,
        start_period_secs: 0,
        max_failures: 0,
    };
    assert!(!signal_restart_missing_health_probe(Some(&art), Some(&hc)));
    assert!(!signal_restart_missing_health_probe(None, None));
}

#[test]
fn trivial_exec_health_probe_recognizes_always_true() {
    for cmd in ["true", "TRUE", " : ", "/bin/true", "/usr/bin/true"] {
        let hc = HealthCheck::Exec {
            command: cmd.into(),
            interval_secs: 5,
            timeout_secs: 0,
            start_period_secs: 0,
            max_failures: 0,
        };
        assert!(trivial_exec_health_probe(Some(&hc)), "cmd={cmd}");
    }
    let real = HealthCheck::Exec {
        command: "curl -f http://127.0.0.1/health".into(),
        interval_secs: 5,
        timeout_secs: 0,
        start_period_secs: 0,
        max_failures: 0,
    };
    assert!(!trivial_exec_health_probe(Some(&real)));
    assert!(!trivial_exec_health_probe(None));
    assert!(!trivial_exec_health_probe(Some(&HealthCheck::Tcp {
        host: "127.0.0.1".into(),
        port: 1,
        interval_secs: 5,
        timeout_secs: 0,
        start_period_secs: 0,
        max_failures: 0,
    })));
}

#[test]
fn location_includes_service_index_and_name() {
    let err = with_program_location(
        anyhow::anyhow!("command: must not be empty"),
        Some("web"),
        Some(2),
    );
    assert_eq!(
        err.to_string(),
        "services[2] (name=web): command: must not be empty"
    );
}

#[test]
fn create_rejects_max_concurrent_over_cap() {
    let dir = tmp_logs();
    let mut req = minimal_create("/bin/true");
    req.max_concurrent = Some(crate::MAX_CONCURRENT_CAP + 1);
    let err = validate_create_program_request(&req, dir.path()).unwrap_err();
    assert!(err.to_string().contains("max_concurrent"), "{err}");
}

#[test]
fn create_rejects_max_queued_over_cap() {
    let dir = tmp_logs();
    let mut req = minimal_create("/bin/true");
    req.max_queued = Some(crate::MAX_QUEUED_CAP + 1);
    let err = validate_create_program_request(&req, dir.path()).unwrap_err();
    assert!(err.to_string().contains("max_queued"), "{err}");
}

#[test]
fn create_accepts_zero_and_defaults() {
    let dir = tmp_logs();
    let mut req = minimal_create("/bin/true");
    req.max_concurrent = Some(0); // 0 means default
    req.max_queued = Some(0);
    validate_create_program_request(&req, dir.path()).unwrap();
}

#[test]
fn update_validates_cron_concurrency_bounds() {
    let dir = tmp_logs();
    let req = UpdateProgramRequest {
        max_concurrent: Some(crate::MAX_CONCURRENT_CAP + 1),
        max_queued: Some(3),
        ..Default::default()
    };
    let err = validate_update_program_request(&req, dir.path()).unwrap_err();
    assert!(err.to_string().contains("max_concurrent"), "{err}");

    let req = UpdateProgramRequest {
        max_concurrent: Some(4),
        max_queued: Some(crate::MAX_QUEUED_CAP + 1),
        ..Default::default()
    };
    let err = validate_update_program_request(&req, dir.path()).unwrap_err();
    assert!(err.to_string().contains("max_queued"), "{err}");
}

#[test]
fn effective_values_normalize() {
    use crate::ProgramConfig;

    let default = ProgramConfig {
        name: "x".into(),
        command: "true".into(),
        ..Default::default()
    };
    assert_eq!(default.max_concurrent_eff(), 1);
    assert_eq!(default.max_queued_eff(), 100);

    let zero = ProgramConfig {
        name: "x".into(),
        command: "true".into(),
        max_concurrent: Some(0),
        max_queued: Some(0),
        ..Default::default()
    };
    assert_eq!(zero.max_concurrent_eff(), 1);
    assert_eq!(zero.max_queued_eff(), 100);

    let capped = ProgramConfig {
        name: "x".into(),
        command: "true".into(),
        max_concurrent: Some(crate::MAX_CONCURRENT_CAP * 2),
        max_queued: Some(crate::MAX_QUEUED_CAP * 2),
        ..Default::default()
    };
    assert_eq!(capped.max_concurrent_eff(), crate::MAX_CONCURRENT_CAP);
    assert_eq!(capped.max_queued_eff(), crate::MAX_QUEUED_CAP);
}

#[test]
fn parse_toml_stack_artifact_timeouts() {
    let toml = r#"
[[services]]
name = "api"
command = "/usr/local/bin/api"

[services.artifact]
source = "https://example.com/api"
checksum = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
destination = "/usr/local/bin/api"
extract = false
restart_policy = "immediate"
download_timeout = 120
verify_timeout = 45
"#;
    let stack = parse_stack_from_str(toml, Path::new("conf/conf.d/api.toml")).unwrap();
    let art = stack.services[0].artifact.as_ref().expect("artifact block");
    assert_eq!(art.download_timeout, 120);
    assert_eq!(art.verify_timeout, 45);
    assert_eq!(art.restart_policy, "immediate");
    assert!(!art.extract);
}

#[test]
fn parse_toml_stack_artifact_timeouts_default_when_omitted() {
    let toml = r#"
[[services]]
name = "api"
command = "/usr/local/bin/api"
artifact = { source = "https://example.com/api", checksum = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", destination = "/usr/local/bin/api" }
"#;
    let stack = parse_stack_from_str(toml, Path::new("api.toml")).unwrap();
    let art = stack.services[0].artifact.as_ref().unwrap();
    assert_eq!(art.download_timeout, 60);
    assert_eq!(art.verify_timeout, 60);
    assert_eq!(art.restart_policy, "immediate");
}

#[test]
fn parse_toml_stack_with_health_check() {
    let toml = r#"
[[services]]
name = "web"
command = "/usr/bin/python3"
args = ["-m", "http.server", "8080"]
env = { PORT = "8080", DEBUG = "1" }
autorestart = "true"
exitcodes = [0, 2]
health_check = { type = "http", url = "http://127.0.0.1:8080/health", interval_secs = 5 }

[[services]]
name = "worker"
command = "/bin/worker"
autorestart = "false"
"#;
    let stack = parse_stack_from_str(toml, Path::new("conf/conf.d/stack.toml")).unwrap();
    assert_eq!(stack.services.len(), 2);
    assert!(!stack.prune);

    let web = &stack.services[0];
    assert_eq!(web.name.as_deref(), Some("web"));
    assert_eq!(web.args, vec!["-m", "http.server", "8080"]);
    assert_eq!(web.env.get("PORT").map(String::as_str), Some("8080"));
    assert_eq!(web.autorestart, crate::AutorestartPolicy::True);
    assert_eq!(web.exitcodes, vec![0, 2]);
    match &web.health_check {
        Some(HealthCheck::Http {
            url, interval_secs, ..
        }) => {
            assert_eq!(url, "http://127.0.0.1:8080/health");
            assert_eq!(*interval_secs, 5);
        }
        other => panic!("unexpected health check: {other:?}"),
    }
    assert_eq!(
        stack.services[1].autorestart,
        crate::AutorestartPolicy::False
    );
}

#[test]
fn parse_toml_stack_via_inline_table() {
    let toml = r#"
[[services]]
name = "db"
command = "/usr/bin/postgres"
health_check = { type = "tcp", host = "127.0.0.1", port = 5432 }
"#;
    let stack = parse_stack_from_str(toml, Path::new("db.toml")).unwrap();
    match &stack.services[0].health_check {
        Some(HealthCheck::Tcp { host, port, .. }) => {
            assert_eq!(host, "127.0.0.1");
            assert_eq!(*port, 5432);
        }
        other => panic!("unexpected health check: {other:?}"),
    }
}

#[test]
fn parse_json_stack_still_supported() {
    let json = r#"{
            "prune": true,
            "services": [
                {"name": "web", "command": "/bin/true", "autorestart": "unexpected"}
            ]
        }"#;
    let stack = parse_stack_from_str(json, Path::new("conf/conf.d/legacy.json")).unwrap();
    assert!(stack.prune);
    assert_eq!(stack.services.len(), 1);
    assert_eq!(
        stack.services[0].autorestart,
        crate::AutorestartPolicy::Unexpected
    );
}

#[test]
fn parse_no_extension_defaults_to_toml() {
    let toml = r#"
[[services]]
name = "cron"
command = "/bin/echo"
"#;
    let stack = parse_stack_from_str(toml, Path::new("conf/conf.d/stack")).unwrap();
    assert_eq!(stack.services.len(), 1);
    assert_eq!(stack.services[0].name.as_deref(), Some("cron"));
}

#[test]
fn example_stack_all_toml_parses() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo/resource/stack_all.toml");
    let content = std::fs::read_to_string(&path).unwrap();
    let stack = parse_stack_from_str(&content, &path).unwrap();
    assert_eq!(stack.services.len(), 4, "{stack:?}");

    let api = stack
        .services
        .iter()
        .find(|s| s.name.as_deref() == Some("web-api"))
        .expect("web-api present");
    assert!(matches!(api.health_check, Some(HealthCheck::Http { .. })));
    assert!(api.hooks.pre_start.is_some(), "hooks pre_start present");
    assert_eq!(api.depends_on, vec!["sys-db"]);
}

#[test]
fn stack_toml_roundtrip_through_export() {
    // Mirrors `super export --format toml`: serialize to TOML, then confirm
    // the output re-parses identically (toml emits nested tables for
    // internally-tagged enums, which the parser accepts).
    let stack = StackApplyRequest {
        services: vec![CreateProgramRequest {
            name: Some("web".into()),
            command: "/bin/true".into(),
            env: HashMap::from([("PORT".into(), "8080".into())]),
            health_check: Some(HealthCheck::Http {
                url: "http://127.0.0.1:8080/health".into(),
                method: Some("GET".into()),
                interval_secs: 5,
                timeout_secs: 0,
                start_period_secs: 0,
                max_failures: 0,
            }),
            ..Default::default()
        }],
        prune: true,
    };
    let s = toml::to_string_pretty(&stack).unwrap();
    let parsed = parse_stack_from_str(&s, Path::new("exported.toml")).unwrap();
    assert_eq!(parsed.prune, stack.prune);
    let a = &parsed.services[0];
    let b = &stack.services[0];
    assert_eq!(a.name, b.name);
    assert_eq!(a.command, b.command);
    assert_eq!(a.env, b.env);
    match (&a.health_check, &b.health_check) {
        (
            Some(HealthCheck::Http {
                url: au,
                method: am,
                interval_secs: ai,
                ..
            }),
            Some(HealthCheck::Http {
                url: bu,
                method: bm,
                interval_secs: bi,
                ..
            }),
        ) => {
            assert_eq!(au, bu);
            assert_eq!(am, bm);
            assert_eq!(ai, bi);
        }
        other => panic!("health check mismatch after round-trip: {other:?}"),
    }
}

#[test]
fn parse_toml_stack_via_nested_table() {
    // Single nested tables are accepted for internally-tagged enums
    // (only [[array.of.tables]] is rejected).
    let toml = r#"
[[services]]
name = "db"
command = "/usr/bin/postgres"

[services.health_check]
type = "tcp"
host = "127.0.0.1"
port = 5432
"#;
    let stack = parse_stack_from_str(toml, Path::new("db.toml")).unwrap();
    match &stack.services[0].health_check {
        Some(HealthCheck::Tcp { host, port, .. }) => {
            assert_eq!(host, "127.0.0.1");
            assert_eq!(*port, 5432);
        }
        other => panic!("unexpected health check: {other:?}"),
    }
}

#[test]
fn toml_parse_error_carries_location() {
    let bad = "[[services]]\nname = \"web\"\ncommand = [1, 2]\n";
    let err = parse_stack_from_str(bad, Path::new("conf/conf.d/stack.toml")).unwrap_err();
    let msg = err.to_string();
    assert!(msg.starts_with("conf/conf.d/stack.toml:"), "{msg}");
    assert!(msg.contains("expected"), "{msg}");
}

// --- dependency graph (references + cycles) ----------------------------

use std::collections::HashSet;

fn known(names: &[&str]) -> HashSet<String> {
    names.iter().map(|s| s.to_string()).collect()
}

#[test]
fn graph_accepts_acyclic_and_self_forward_ok() {
    let k = known(&["web", "nginx", "db"]);
    // nginx -> web -> db (start order), plus db with no deps.
    let graph = [
        ("nginx".to_string(), vec!["web".to_string()]),
        ("web".to_string(), vec!["db".to_string()]),
        ("db".to_string(), vec![]),
    ];
    let refs: Vec<(String, &[String])> = graph
        .iter()
        .map(|(n, d)| (n.clone(), d.as_slice()))
        .collect();
    validate_dependency_graph(&k, &refs).unwrap();
}

#[test]
fn graph_rejects_two_node_cycle() {
    let k = known(&["a", "b"]);
    let graph = [
        ("a".to_string(), vec!["b".to_string()]),
        ("b".to_string(), vec!["a".to_string()]),
    ];
    let refs: Vec<(String, &[String])> = graph
        .iter()
        .map(|(n, d)| (n.clone(), d.as_slice()))
        .collect();
    let err = validate_dependency_graph(&k, &refs).unwrap_err();
    assert!(err.to_string().contains("cycle"), "{err}");
    assert!(
        err.to_string().contains('a') && err.to_string().contains('b'),
        "{err}"
    );
}

#[test]
fn graph_rejects_self_dependency() {
    let k = known(&["a"]);
    let graph = [("a".to_string(), vec!["a".to_string()])];
    let refs: Vec<(String, &[String])> = graph
        .iter()
        .map(|(n, d)| (n.clone(), d.as_slice()))
        .collect();
    let err = validate_dependency_graph(&k, &refs).unwrap_err();
    assert!(err.to_string().contains("themselves"), "{err}");
}

#[test]
fn graph_reference_error_takes_precedence_and_lists_names() {
    let k = known(&["a"]);
    let graph = [("a".to_string(), vec!["ghost".to_string(), "zz".to_string()])];
    // a -> ghost, a -> zz; nothing cyclical, but refs are dangling.
    let refs: Vec<(String, &[String])> = graph
        .iter()
        .map(|(n, d)| (n.clone(), d.as_slice()))
        .collect();
    let err = validate_dependency_graph(&k, &refs).unwrap_err();
    assert!(err.to_string().contains("unknown service"), "{err}");
    assert!(
        err.to_string().contains("ghost") && err.to_string().contains("zz"),
        "{err}"
    );
}
