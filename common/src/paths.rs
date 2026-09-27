use std::env;
use std::path::{Path, PathBuf};

/// Resolve Super instance root: `SUPER_ROOT` → exe-relative layout → cwd.
///
/// Shared by superd and licensed plugins so config paths stay consistent.
pub fn resolve_super_root() -> PathBuf {
    if let Some(root) = env_super_root() {
        return root;
    }

    if let Ok(exe_path) = env::current_exe()
        && let Some(bin_dir) = exe_path.parent()
        && let Some(root) = bin_dir.parent()
        && root.join("bin").exists()
    {
        return root.to_path_buf();
    }

    PathBuf::from(".")
}

/// Resolve instance root for config tooling (`super check`, etc.).
///
/// Order: `SUPER_ROOT` → layout inferred from `config_path` → [`resolve_super_root`].
///
/// Typical layouts:
/// - `$ROOT/conf/super.toml` → `$ROOT`
/// - `$ROOT/super.toml` → `$ROOT`
pub fn resolve_super_root_for_config(config_path: &Path) -> PathBuf {
    if let Some(root) = env_super_root() {
        return root;
    }
    if let Some(root) = infer_super_root_from_config(config_path) {
        return root;
    }
    resolve_super_root()
}

/// Resolve a storage path relative to the instance root.
///
/// Absolute paths are returned unchanged; relative paths (including `./…`) join
/// under `root` so daemon logs and data never depend on process CWD.
pub fn resolve_storage_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn env_super_root() -> Option<PathBuf> {
    env::var("SUPER_ROOT")
        .ok()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
}

fn infer_super_root_from_config(config_path: &Path) -> Option<PathBuf> {
    let abs = if config_path.is_absolute() {
        config_path.to_path_buf()
    } else {
        env::current_dir().ok()?.join(config_path)
    };
    let parent = abs.parent()?;
    if parent
        .file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("conf"))
    {
        return parent.parent().map(|p| p.to_path_buf());
    }
    Some(parent.to_path_buf())
}

#[cfg(test)]
#[path = "tests/paths_tests.rs"]
mod tests;
