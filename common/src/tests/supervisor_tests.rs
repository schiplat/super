use super::*;

fn parse(input: &str) -> StackDraft {
    SupervisorFormat
        .parse(input, &ParseCtx::default())
        .expect("parse ok")
}

#[test]
fn detect_rejects_toml_and_accepts_ini() {
    assert!(!SupervisorFormat.detect("[[services]]\nname = \"web\"\n"));
    assert!(SupervisorFormat.detect("[program:web]\ncommand=/bin/x\n"));
    assert!(SupervisorFormat.detect("[supervisord]\nnodaemon=true\n"));
}

#[test]
fn basic_program_maps_fields() {
    let d = parse(
        "[program:web]\n\
             command=/usr/bin/node server.js --port 8080\n\
             directory=/srv/web\n\
             user=www-data\n\
             autostart=true\n\
             autorestart=unexpected\n\
             startsecs=5\n\
             stopwaitsecs=30\n\
             priority=100\n\
             environment=PORT=\"8080\",DEBUG=\"1\"\n",
    );
    assert_eq!(d.services.len(), 1);
    let s = &d.services[0];
    assert_eq!(s.name.as_deref(), Some("web"));
    assert_eq!(s.command, "/usr/bin/node");
    assert_eq!(s.args, vec!["server.js", "--port", "8080"]);
    assert_eq!(s.cwd.as_deref(), Some("/srv/web"));
    assert_eq!(s.user.as_deref(), Some("www-data"));
    assert!(s.autostart);
    assert_eq!(s.autorestart, AutorestartPolicy::Unexpected);
    assert_eq!(s.startsecs, 5);
    assert_eq!(s.stopsecs, Some(30));
    assert_eq!(s.priority, 100);
    assert_eq!(s.env.get("PORT").map(String::as_str), Some("8080"));
    assert_eq!(s.env.get("DEBUG").map(String::as_str), Some("1"));
    assert!(d.warnings.is_empty());
}

#[test]
fn missing_command_is_error() {
    let err = SupervisorFormat
        .parse("[program:web]\nuser=x\n", &ParseCtx::default())
        .unwrap_err();
    assert!(err.to_string().contains("missing `command`"));
}

#[test]
fn infra_sections_skipped_silently() {
    let d = parse(
        "[supervisord]\n\
             nodaemon=true\n\
             logfile=/var/log/supervisord.log\n\
             \n\
             [program:web]\n\
             command=/bin/sleep 30\n\
             \n\
             [unix_http_server]\n\
             file=/tmp/s.sock\n\
             \n\
             [supervisorctl]\n\
             serverurl=unix:///tmp/s.sock\n\
             \n\
             [rpcinterface:supervisor]\n\
             supervisor.rpcinterface_factory=supervisor.rpcinterface:make_main_rpcinterface\n",
    );
    assert_eq!(d.services.len(), 1);
    assert!(d.warnings.is_empty());
}

#[test]
fn unknown_section_warns() {
    let d = parse(
        "[program:web]\ncommand=/bin/sleep 30\n\n[eventlistener:mailer]\ncommand=/bin/mail\n",
    );
    assert_eq!(d.services.len(), 1);
    assert!(
        d.warnings
            .iter()
            .any(|w| w.section.contains("eventlistener"))
    );
}

#[test]
fn autorestart_true_false_unexpected() {
    for (src, want) in [
        ("false", AutorestartPolicy::False),
        ("0", AutorestartPolicy::False),
        ("true", AutorestartPolicy::True),
        ("unexpected", AutorestartPolicy::Unexpected),
    ] {
        let d = parse(&format!(
            "[program:a]\ncommand=/bin/sleep 1\nautorestart={src}\n"
        ));
        assert_eq!(d.services[0].autorestart, want, "autorestart={src}");
    }
}

#[test]
fn exitcodes_csv_parses() {
    let d = parse("[program:a]\ncommand=/bin/sleep 1\nexitcodes=0,2\n");
    assert_eq!(d.services[0].exitcodes, vec![0, 2]);
}

#[test]
fn numprocs_with_template() {
    let d = parse(
        "[program:worker]\n\
             command=/bin/worker\n\
             numprocs=4\n\
             process_name=%(program_name)s_%(process_num)02d\n",
    );
    let s = &d.services[0];
    assert_eq!(s.numprocs, 4);
    assert_eq!(s.process_name.as_deref(), Some("{name}_{num}"));
    assert!(d.warnings.is_empty());
}

#[test]
fn redirect_stderr_merges_streams() {
    let d = parse(
        "[program:a]\ncommand=/bin/sleep 1\nredirect_stderr=true\nstdout_logfile=/var/log/a.log\n",
    );
    let s = &d.services[0];
    assert_eq!(s.stdout_logfile, s.stderr_logfile);
}

#[test]
fn log_rotation_keys_collapse_to_one_warning() {
    let d = parse(
        "[program:a]\n\
             command=/bin/sleep 1\n\
             stdout_logfile_maxbytes=50MB\n\
             stdout_logfile_backups=10\n\
             stdout_logfile_datefmt=%%Y-%%m-%%d\n",
    );
    assert_eq!(d.warnings.len(), 1);
    assert!(d.warnings[0].message.contains("rotation is global"));
    assert!(
        d.warnings[0]
            .message
            .contains("stdout_logfile_maxbytes=50MB")
    );
    assert!(d.warnings[0].message.contains("stdout_logfile_datefmt"));
}

#[test]
fn stopasgroup_needs_no_warning() {
    let d = parse("[program:a]\ncommand=/bin/sleep 1\nstopasgroup=true\nkillasgroup=true\n");
    assert!(d.warnings.is_empty());
}

#[test]
fn comments_and_continuations() {
    let d = parse(
        "# top comment\n\
             ; also comment\n\
             [program:a]\n\
             ; inner comment\n\
             command=/bin/echo a b \\\n  c d\n",
    );
    let s = &d.services[0];
    assert_eq!(s.command, "/bin/echo");
    assert_eq!(s.args, vec!["a", "b", "c", "d"]);
}

#[test]
fn env_placeholder_warns_and_expands() {
    // SAFETY(test): single-threaded test binary section; setting a unique
    // test var is not read elsewhere.
    unsafe { std::env::set_var("IMPORT_TEST_VAR", "hello") };
    let d = parse(
        "[program:a]\ncommand=/bin/sleep 1\nenvironment=GREET=\"%(ENV_IMPORT_TEST_VAR)s world\"\n",
    );
    assert_eq!(
        d.services[0].env.get("GREET").map(String::as_str),
        Some("hello world")
    );
    assert!(d.warnings.iter().any(|w| w.message.contains("%(ENV_")));
}

#[test]
fn group_maps_members() {
    let d = parse(
        "[program:web]\ncommand=/bin/sleep 1\n\n[program:db]\ncommand=/bin/sleep 2\n\n[group:app]\nprograms=web,db\n",
    );
    assert_eq!(d.services.len(), 2);
    assert!(d.services.iter().all(|s| s.group.as_deref() == Some("app")));
}

#[test]
fn umask_and_friends_warn() {
    let d = parse("[program:a]\ncommand=/bin/sleep 1\numask=022\nserverurl=unix:///x\n");
    assert!(d.warnings.iter().any(|w| w.message.contains("umask")));
    assert!(d.warnings.iter().any(|w| w.message.contains("serverurl")));
}

#[test]
fn stopsignal_non_term_warns_with_hint() {
    let d = parse("[program:a]\ncommand=/bin/sleep 1\nstopsignal=QUIT\n");
    let w = d
        .warnings
        .iter()
        .find(|w| w.message.contains("stopsignal=QUIT"))
        .expect("stopsignal warning");
    assert!(w.message.contains("super signal a quit"));
}

#[test]
fn quoted_command_args_survive() {
    let d = parse("[program:a]\ncommand=/bin/echo \"hello world\" 'x y'\n");
    let s = &d.services[0];
    assert_eq!(s.args, vec!["hello world", "x y"]);
}

#[test]
fn bare_command_without_args() {
    let d = parse("[program:a]\ncommand=/usr/sbin/nginx\n");
    assert_eq!(d.services[0].command, "/usr/sbin/nginx");
    assert!(d.services[0].args.is_empty());
}

#[test]
fn include_expands_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    std::fs::write(root.join("a.conf"), "[program:a]\ncommand=/bin/sleep 1\n").unwrap();
    std::fs::write(root.join("b.conf"), "[program:b]\ncommand=/bin/sleep 2\n").unwrap();
    let main = format!(
        "[include]\nfiles={}/a.conf {}/b.conf\n",
        root.display(),
        root.display()
    );
    let d = SupervisorFormat
        .parse(
            &main,
            &ParseCtx {
                here_dir: Some(root.to_path_buf()),
                allow_include: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(d.services.len(), 2);
}

#[test]
fn include_glob_expands() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    std::fs::write(root.join("w1.conf"), "[program:w1]\ncommand=/bin/sleep 1\n").unwrap();
    std::fs::write(root.join("w2.conf"), "[program:w2]\ncommand=/bin/sleep 2\n").unwrap();
    let main = format!("[include]\nfiles={}/w*.conf\n", root.display());
    let d = SupervisorFormat
        .parse(
            &main,
            &ParseCtx {
                here_dir: Some(root.to_path_buf()),
                allow_include: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(d.services.len(), 2);
}

#[test]
fn missing_include_is_hard_error() {
    let err = SupervisorFormat
        .parse(
            "[include]\nfiles=/definitely/not/here/x.conf\n",
            &ParseCtx {
                here_dir: Some(PathBuf::from("/tmp")),
                allow_include: true,
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(err.to_string().contains("[include]"));
}
