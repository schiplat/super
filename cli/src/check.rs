use colored::Colorize;
use common::config::{
    LEGACY_WEBHOOK_SECTION_MSG, ServerConfig, legacy_webhook_section_present, resolve_license_key,
};
use common::is_loopback_bind_host;
use common::resolve_super_root_for_config;
use common::{
    StackApplyRequest, format_serde_json_error, licensed_deployment_intent, resolve_license_strict,
    scan_plugin_stems, validate_create_program_request, verify_license_for_superd,
    with_program_location,
};
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};

/// Run configuration check command
pub fn run(file_path: Option<PathBuf>) -> anyhow::Result<()> {
    // 1. Locate config file
    let path = resolve_config_path(file_path)?;
    println!(
        "Checking configuration at: {}",
        path.display().to_string().cyan()
    );

    // 2. Read and parse TOML
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            return Err(e.into());
        }
    };

    let config: ServerConfig = match toml::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("TOML syntax error in {}: {}", path.display(), e);
            return Err(anyhow::anyhow!("invalid super.toml: {e}"));
        }
    };

    println!("   Syntax:      {}", "OK".green());

    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    if legacy_webhook_section_present(&content) {
        errors.push(LEGACY_WEBHOOK_SECTION_MSG.to_string());
    }
    if stray_program_tables_in_toml(&content) {
        errors.push(
            "[[program]] / [[programs]] in super.toml is ignored — programs load from \
             [include] JSON (conf/conf.d/*.json), the API, or data/snapshot.json."
                .into(),
        );
    }

    // 3. Check server config (port availability and privileges)
    let bind_addr = format!("{}:{}", config.server.host, config.server.port);
    print!("   Server Addr: {} ... ", bind_addr);

    // Try binding the port to detect conflicts
    match TcpListener::bind(&bind_addr) {
        Ok(_) => {
            print!("{}", "Available".green());
        }
        Err(e) => {
            print!("{}", "Occupied".red());
            errors.push(format!(
                "Port {} is likely in use: {}",
                config.server.port, e
            ));
        }
    }

    // Privileged ports (<1024) require root
    if config.server.port < 1024 && config.server.port != 0 {
        #[cfg(unix)]
        if unsafe { libc::geteuid() } != 0 {
            print!(" {}", "(Non-Root Warning)".yellow());
            warnings.push(format!(
                "Port {} usually requires root privileges",
                config.server.port
            ));
        }
    }
    println!();

    let licensed_ready = check_licensed_deployment(&path, &config, &mut errors, &mut warnings);

    if !licensed_ready
        && !is_loopback_bind_host(&config.server.host)
        && !config.server.allow_insecure_public_bind
    {
        errors.push(format!(
            "Server binds to {} without loopback isolation. \
             Set allow_insecure_public_bind = true, bind to 127.0.0.1, \
             or load the security plugin at runtime.",
            config.server.host
        ));
    }

    // 4. Check log directory (write permission)
    let log_dir = &config.storage.log_dir;
    print!("   Log Dir:     {:?} ... ", log_dir);

    if log_dir.exists() {
        if log_dir.is_dir() {
            if is_writable(log_dir) {
                println!("{}", "Writable".green());
            } else {
                println!("{}", "Permission Denied".red());
                errors.push(format!("Log dir {:?} exists but is NOT writable", log_dir));
            }
        } else {
            println!("{}", "Error".red());
            errors.push(format!(
                "Log path {:?} exists but is not a directory",
                log_dir
            ));
        }
    } else {
        // Directory missing; check whether parent allows creation
        let (writable, ancestor) = check_ancestor_writable(log_dir);
        if writable {
            println!("{}", "OK (Writable)".green());
        } else {
            println!("{}", "Permission Denied".red());
            errors.push(format!(
                "Cannot create log dir under read-only ancestor: {:?}",
                ancestor
            ));
        }
    }

    // 5. Check data file (snapshot storage)
    let data_file = &config.storage.data_file;
    print!("   Data File:   {:?} ... ", data_file);

    if data_file.exists() {
        if data_file.is_file() {
            // File exists; check writability
            if is_writable(data_file) {
                println!("{}", "Writable".green());
            } else {
                // Check read-only attribute
                if let Ok(m) = fs::metadata(data_file) {
                    if m.permissions().readonly() {
                        println!("{}", "Read-only".red());
                        errors.push(format!("Data file {:?} is read-only", data_file));
                    } else {
                        println!("{}", "Permission Denied".red());
                        errors.push(format!("Data file {:?} is not writable", data_file));
                    }
                }
            }
        } else {
            println!("{}", "Error".red());
            errors.push(format!(
                "Data path {:?} exists but is not a file",
                data_file
            ));
        }
    } else {
        // File missing; check whether parent allows creation
        if let Some(parent) = data_file.parent() {
            let (writable, ancestor) = check_ancestor_writable(parent);
            if writable {
                println!("{}", "OK (Writable)".green());
            } else {
                println!("{}", "Permission Denied".red());
                errors.push(format!(
                    "Cannot create data file under read-only ancestor: {:?}",
                    ancestor
                ));
            }
        } else {
            println!("{}", "Error".red());
            errors.push(format!("Invalid data file path: {:?}", data_file));
        }
    }

    // 6. Include globs + JSON stack syntax (daemon skips invalid include files)
    let root = resolve_super_root_for_config(&path);
    check_include_stacks(
        &root,
        &config.include.files,
        &config.storage.log_dir,
        &mut errors,
        &mut warnings,
    );

    // 7. Print summary
    if !warnings.is_empty() {
        println!("\n{}", "Warnings:".yellow().bold());
        for w in warnings {
            println!("   - {}", w);
        }
    }

    if errors.is_empty() {
        println!("\n{}", "Configuration is VALID".green().bold());
        Ok(())
    } else {
        println!("\n{}", "Found Errors:".red().bold());
        for e in errors {
            println!("   - {}", e);
        }
        Err(anyhow::anyhow!("Configuration check failed"))
    }
}

/// Check whether a path is writable.
/// For files, try append open; for directories, create a temp file.
fn is_writable(path: &Path) -> bool {
    if path.is_dir() {
        let test_file = path.join(".perm_check_tmp");
        if fs::write(&test_file, "").is_ok() {
            fs::remove_file(&test_file).ok();
            return true;
        }
    } else if fs::OpenOptions::new().append(true).open(path).is_ok() {
        return true;
    }
    false
}

/// Walk up to the nearest existing ancestor and check write permission.
/// Returns (writable, existing_ancestor_path).
fn check_ancestor_writable(target_path: &Path) -> (bool, PathBuf) {
    let mut current = target_path.to_path_buf();

    // Path does not exist; walk up
    while !current.exists() {
        if let Some(parent) = current.parent() {
            current = parent.to_path_buf();
        } else {
            // At root or invalid path; fall back to current directory
            return (is_writable(Path::new(".")), Path::new(".").to_path_buf());
        }
    }

    // Found an existing directory; check writability
    (is_writable(&current), current)
}

/// `[[program]]` in super.toml is not part of `ServerConfig` and is silently ignored.
fn stray_program_tables_in_toml(content: &str) -> bool {
    content.lines().any(|line| {
        let header = line.split('#').next().unwrap_or("").trim();
        header == "[[program]]" || header == "[[programs]]"
    })
}

/// Resolve include globs like superd (`process_includes`) and parse each JSON stack.
fn check_include_stacks(
    root: &Path,
    patterns: &[String],
    log_dir: &Path,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    if patterns.is_empty() {
        return;
    }
    println!("   Includes:    {} pattern(s)", patterns.len());
    for pattern in patterns {
        let pattern_path = Path::new(pattern);
        let full_pattern = if pattern_path.is_relative() {
            root.join(pattern).to_string_lossy().into_owned()
        } else if pattern_path.starts_with(root) {
            pattern.clone()
        } else {
            warnings.push(format!(
                "Skipping include pattern outside SUPER_ROOT ({root}): {pattern}",
                root = root.display()
            ));
            println!("     - '{pattern}': skipped (outside instance root)");
            continue;
        };

        match glob::glob(&full_pattern) {
            Err(_) => errors.push(format!("Invalid include glob: {pattern}")),
            Ok(paths) => {
                let matched: Vec<_> = paths.flatten().collect();
                if matched.is_empty() {
                    warnings.push(format!("Include glob matched no files: {pattern}"));
                    println!("     - '{pattern}': no files");
                    continue;
                }
                println!("     - '{pattern}': {} file(s)", matched.len());
                for entry in matched {
                    match fs::read_to_string(&entry) {
                        Err(e) => {
                            errors.push(format!("Cannot read include {}: {e}", entry.display()))
                        }
                        Ok(body) => match serde_json::from_str::<StackApplyRequest>(&body) {
                            Ok(stack) => {
                                if stack.services.is_empty() {
                                    warnings.push(format!(
                                        "{} has an empty services array",
                                        entry.display()
                                    ));
                                } else {
                                    let mut invalid = false;
                                    for (i, svc) in stack.services.iter().enumerate() {
                                        if let Err(e) =
                                            validate_create_program_request(svc, log_dir)
                                        {
                                            invalid = true;
                                            errors.push(format!(
                                                "{}: {}",
                                                entry.display(),
                                                with_program_location(
                                                    e,
                                                    svc.name.as_deref(),
                                                    Some(i)
                                                )
                                            ));
                                        }
                                    }
                                    if !invalid {
                                        println!(
                                            "       {}: {} service(s) OK",
                                            entry.display(),
                                            stack.services.len()
                                        );
                                    }
                                }
                            }
                            Err(e) => errors
                                .push(format_serde_json_error(&entry.display().to_string(), &e)),
                        },
                    }
                }
            }
        }
    }
}

/// Resolve default config file path
fn resolve_config_path(user_path: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    if let Some(p) = user_path {
        return Ok(p);
    }

    let candidates = ["super.toml", "conf/super.toml", "/etc/super/super.toml"];

    for c in candidates {
        let p = PathBuf::from(c);
        if p.exists() {
            return Ok(p);
        }
    }

    Err(anyhow::anyhow!(
        "Config file not found. Please specify with --file"
    ))
}

/// When a valid license is configured, mirror superd startup requirements.
/// Returns `true` when licensed mode is expected to start successfully (security + auth).
fn check_licensed_deployment(
    config_path: &Path,
    config: &ServerConfig,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> bool {
    let Ok(Some(license_key)) = resolve_license_key(config_path) else {
        return false;
    };

    let Ok((claims, _status)) = verify_license_for_superd(&license_key) else {
        let plugins_dir = resolve_super_root_for_config(config_path).join("plugins");
        let installed = scan_plugin_stems(&plugins_dir);
        let strict = resolve_license_strict(config_path).unwrap_or(false);
        let intent = licensed_deployment_intent(config, &installed);
        let base = "License key present but verification failed";
        if strict || intent {
            errors.push(format!(
                "{base} — superd will refuse startup (strict mode or licensed deployment signals: \
                 plugins on disk, auth_secret, or non-loopback bind). Fix [license].key or remove licensed-only config."
            ));
        } else {
            warnings.push(format!(
                "{base} — superd will run in OSS mode without plugins. Set [license].strict = true to refuse startup instead."
            ));
        }
        return false;
    };

    let plugins_dir = resolve_super_root_for_config(config_path).join("plugins");
    let req_errors = licensed_requirement_errors(
        &claims.grants,
        &plugins_dir,
        config.auth_secret.as_deref(),
        config.server.allow_insecure_public_bind,
    );
    let ok = req_errors.is_empty();
    errors.extend(req_errors);
    ok
}

/// Structural licensed checks after a key has verified successfully.
fn licensed_requirement_errors(
    plugins_in_claims: &[String],
    plugins_dir: &Path,
    auth_secret: Option<&str>,
    allow_insecure_public_bind: bool,
) -> Vec<String> {
    let mut errors = Vec::new();

    if !plugins_in_claims.iter().any(|p| p == "security") {
        errors.push(
            "Licensed deployment requires 'security' in license claims (included with every subscription).".into(),
        );
    }

    let has_security = ["security.so", "security.dylib"]
        .iter()
        .any(|name| plugins_dir.join(name).is_file());
    if !has_security {
        errors.push(format!(
            "Licensed deployment requires {}/security.so (or security.dylib)",
            plugins_dir.display()
        ));
    }

    if auth_secret.is_none_or(|s| s.trim().is_empty()) {
        errors.push(
            "Licensed deployment requires auth_secret in super.toml (or via environment).".into(),
        );
    }

    if allow_insecure_public_bind {
        errors.push(
            "allow_insecure_public_bind is not used when a valid license is configured — remove it or use OSS mode without a license key.".into(),
        );
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn licensed_ok_when_security_plugin_and_auth_present() {
        let dir = std::env::temp_dir().join(format!(
            "super-check-ok-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let plugins = dir.join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        fs::write(plugins.join("security.dylib"), b"fake").unwrap();

        let errors = licensed_requirement_errors(
            &["security".into(), "ui".into()],
            &plugins,
            Some("secret"),
            false,
        );
        assert!(errors.is_empty(), "{errors:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn licensed_errors_without_security_plugin_or_auth() {
        let dir = std::env::temp_dir().join(format!(
            "super-check-err-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let plugins = dir.join("plugins");
        fs::create_dir_all(&plugins).unwrap();

        let errors = licensed_requirement_errors(&["ui".into()], &plugins, Some("  "), true);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("security' in license claims")),
            "{errors:?}"
        );
        assert!(
            errors.iter().any(|e| e.contains("security.so")),
            "{errors:?}"
        );
        assert!(
            errors.iter().any(|e| e.contains("auth_secret")),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.contains("allow_insecure_public_bind")),
            "{errors:?}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stray_program_tables_detected() {
        assert!(stray_program_tables_in_toml("[[program]]\nname = \"x\"\n"));
        assert!(stray_program_tables_in_toml("[[programs]]\n"));
        assert!(!stray_program_tables_in_toml("# [[program]]\n[server]\n"));
        assert!(!stray_program_tables_in_toml("[include]\nfiles = []\n"));
    }

    #[test]
    fn include_json_syntax_error_is_reported() {
        let dir = std::env::temp_dir().join(format!(
            "super-check-inc-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let confd = dir.join("conf").join("conf.d");
        fs::create_dir_all(&confd).unwrap();
        fs::write(confd.join("bad.json"), "{ not json ").unwrap();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        check_include_stacks(
            &dir,
            &["conf/conf.d/*.json".into()],
            &dir.join("logs"),
            &mut errors,
            &mut warnings,
        );
        assert!(
            errors
                .iter()
                .any(|e| e.contains("bad.json:") && e.contains(":1:")),
            "{errors:?}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn include_json_valid_stack_ok() {
        let dir = std::env::temp_dir().join(format!(
            "super-check-inc-ok-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let confd = dir.join("conf").join("conf.d");
        fs::create_dir_all(&confd).unwrap();
        fs::write(
            confd.join("ok.json"),
            r#"{"services":[{"name":"a","command":"/bin/true"}]}"#,
        )
        .unwrap();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        check_include_stacks(
            &dir,
            &["conf/conf.d/*.json".into()],
            &dir.join("logs"),
            &mut errors,
            &mut warnings,
        );
        assert!(errors.is_empty(), "{errors:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn include_json_empty_command_is_reported() {
        let dir = std::env::temp_dir().join(format!(
            "super-check-inc-empty-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let confd = dir.join("conf").join("conf.d");
        fs::create_dir_all(&confd).unwrap();
        fs::write(
            confd.join("empty.json"),
            r#"{"services":[{"name":"a","command":"  "}]}"#,
        )
        .unwrap();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        check_include_stacks(
            &dir,
            &["conf/conf.d/*.json".into()],
            &dir.join("logs"),
            &mut errors,
            &mut warnings,
        );
        assert!(
            errors.iter().any(|e| {
                e.contains("services[0] (name=a)") && e.contains("command: must not be empty")
            }),
            "{errors:?}"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
