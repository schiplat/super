use colored::Colorize;
use common::config::{
    LEGACY_WEBHOOK_SECTION_MSG, ServerConfig, legacy_webhook_section_present, resolve_license_key,
};
use common::is_loopback_bind_host;
use common::resolve_super_root_for_config;
use common::verify_license_for_superd;
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
            eprintln!("TOML Syntax Error: {}", e);
            return Err(anyhow::anyhow!("Invalid config format"));
        }
    };

    println!("   Syntax:      {}", "OK".green());

    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    if legacy_webhook_section_present(&content) {
        errors.push(LEGACY_WEBHOOK_SECTION_MSG.to_string());
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

    // 6. Check include config (glob patterns)
    if !config.include.files.is_empty() {
        println!(
            "   Includes:    Found {} patterns",
            config.include.files.len()
        );
        for pattern in &config.include.files {
            match glob::glob(pattern) {
                Err(_) => errors.push(format!("Invalid glob pattern: {}", pattern)),
                Ok(paths) => {
                    let count = paths.count();
                    if count == 0 {
                        println!("     - '{}': No matching files (Warning)", pattern);
                    } else {
                        println!("     - '{}': Matches {} files", pattern, count);
                    }
                }
            }
        }
    }

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
        warnings.push(
            "License key present but verification failed — superd will run in OSS mode".into(),
        );
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
}
