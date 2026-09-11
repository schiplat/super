//! Resolve the OSS admin secret: config override, or `$SUPER_ROOT/data/auth.key`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use rand::RngCore;

/// Where the active secret came from (for startup logging).
#[derive(Debug, Clone)]
pub enum AuthSecretSource {
    /// Explicit `auth_secret` in `conf/super.toml`.
    Config,
    /// Loaded from an existing `data/auth.key`.
    File { path: PathBuf },
    /// Freshly generated and written to `data/auth.key`.
    Generated { path: PathBuf },
}

/// Resolve the admin Bearer secret.
///
/// Priority: non-empty `auth_secret` in config → else load/create `key_path`.
/// Generated secrets are 32 CSPRNG bytes, hex-encoded (64 chars).
pub fn resolve_auth_secret(
    config_secret: Option<&str>,
    key_path: &Path,
) -> anyhow::Result<(String, AuthSecretSource)> {
    if let Some(s) = config_secret.map(str::trim).filter(|s| !s.is_empty()) {
        return Ok((s.to_string(), AuthSecretSource::Config));
    }
    load_or_create_key_file(key_path)
}

fn load_or_create_key_file(key_path: &Path) -> anyhow::Result<(String, AuthSecretSource)> {
    if key_path.is_file() {
        let raw = fs::read_to_string(key_path)
            .map_err(|e| anyhow::anyhow!("failed to read auth key {}: {e}", key_path.display()))?;
        let secret = raw.trim().to_string();
        if secret.is_empty() {
            anyhow::bail!(
                "auth key file {} is empty — delete it to regenerate, or set auth_secret in conf/super.toml",
                key_path.display()
            );
        }
        return Ok((
            secret,
            AuthSecretSource::File {
                path: key_path.to_path_buf(),
            },
        ));
    }

    if let Some(parent) = key_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("failed to create {}: {e}", parent.display()))?;
    }

    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let secret = hex::encode(bytes);

    write_key_file(key_path, &secret)?;

    Ok((
        secret,
        AuthSecretSource::Generated {
            path: key_path.to_path_buf(),
        },
    ))
}

fn write_key_file(path: &Path, secret: &str) -> anyhow::Result<()> {
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts
        .open(path)
        .map_err(|e| anyhow::anyhow!("failed to create auth key {}: {e}", path.display()))?;
    writeln!(file, "{secret}")
        .map_err(|e| anyhow::anyhow!("failed to write auth key {}: {e}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn config_secret_wins() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.key");
        let (s, src) = resolve_auth_secret(Some("from-config"), &path).unwrap();
        assert_eq!(s, "from-config");
        assert!(matches!(src, AuthSecretSource::Config));
        assert!(!path.exists());
    }

    #[test]
    fn generates_then_reloads_same_key() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.key");
        let (s1, src1) = resolve_auth_secret(None, &path).unwrap();
        assert!(matches!(src1, AuthSecretSource::Generated { .. }));
        assert_eq!(s1.len(), 64);
        assert!(path.is_file());

        let (s2, src2) = resolve_auth_secret(None, &path).unwrap();
        assert!(matches!(src2, AuthSecretSource::File { .. }));
        assert_eq!(s1, s2);
    }

    #[test]
    fn empty_config_falls_through_to_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("auth.key");
        let (s, _) = resolve_auth_secret(Some("  "), &path).unwrap();
        assert_eq!(s.len(), 64);
    }
}
