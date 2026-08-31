use common::{ProgramConfig, ProgramEventRecord};
use std::collections::HashMap;
use std::path::Path;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

/// Save with backup rotation
pub async fn save(path: &Path, data: &HashMap<Uuid, ProgramConfig>) -> anyhow::Result<()> {
    // 1. Backup existing file as .bak
    if path.exists() {
        let backup_path = path.with_extension("json.bak");
        // Ignore backup failure (e.g. permissions); log only, do not block save
        if let Err(e) = tokio::fs::copy(path, &backup_path).await {
            tracing::warn!("Failed to create config backup: {}", e);
        }
    }

    // 2. Serialize
    let content = serde_json::to_string_pretty(data)?;

    // 3. Write temp file
    let tmp_path = path.with_extension("tmp");
    let mut file = tokio::fs::File::create(&tmp_path).await?;
    file.write_all(content.as_bytes()).await?;

    // Hardening: mode 600 so other users cannot read sensitive snapshot data
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = file.metadata().await?.permissions();
        perms.set_mode(0o600);
        file.set_permissions(perms).await?;
    }

    file.flush().await?;

    // 4. Atomic rename to main file
    tokio::fs::rename(tmp_path, path).await?;

    Ok(())
}

/// Load with automatic recovery from backup
pub async fn load_with_recovery(path: &Path) -> anyhow::Result<HashMap<Uuid, ProgramConfig>> {
    // A. Try primary file
    match load_internal(path).await {
        Ok(data) => return Ok(data),
        Err(e) => {
            if !path.exists() {
                tracing::warn!("No snapshot at {:?}; starting with empty state", path);
                return Ok(HashMap::new());
            }
            tracing::error!("Failed to load primary config {:?}: {}", path, e);
        }
    }

    // B. Primary failed; try backup
    let backup_path = path.with_extension("json.bak");
    if backup_path.exists() {
        tracing::warn!("Attempting to recover from backup: {:?}", backup_path);
        match load_internal(&backup_path).await {
            Ok(data) => {
                tracing::info!("Successfully recovered state from backup!");
                return Ok(data);
            }
            Err(e) => {
                tracing::error!("Backup file is also corrupted: {}", e);
            }
        }
    }

    // C. Missing or both corrupted
    // Propagate error; let caller decide crash vs ignore.
    // First boot (no files) is handled inside load_internal.
    if !path.exists() && !backup_path.exists() {
        // Fresh install
        return Ok(HashMap::new());
    }

    Err(anyhow::anyhow!(
        "All configuration files are corrupted. Please check {:?} manually.",
        path
    ))
}

// Internal load helper
async fn load_internal(path: &Path) -> anyhow::Result<HashMap<Uuid, ProgramConfig>> {
    let content = tokio::fs::read_to_string(path).await?;
    if content.trim().is_empty() {
        return Ok(HashMap::new());
    }
    let data = serde_json::from_str(&content)?;
    Ok(data)
}

/// Persist lifecycle event history (atomic write with `.bak` rotation).
pub async fn save_events(
    path: &Path,
    data: &HashMap<Uuid, Vec<ProgramEventRecord>>,
) -> anyhow::Result<()> {
    // Skip creating an empty events file on first boot / when nothing changed.
    if data.is_empty() && !path.exists() {
        return Ok(());
    }

    if path.exists() {
        let backup_path = path.with_extension("json.bak");
        if let Err(e) = tokio::fs::copy(path, &backup_path).await {
            tracing::warn!("Failed to create events backup: {}", e);
        }
    }

    let content = serde_json::to_string_pretty(data)?;
    let tmp_path = path.with_extension("tmp");
    let mut file = tokio::fs::File::create(&tmp_path).await?;
    file.write_all(content.as_bytes()).await?;

    // mode 600: event history may carry sensitive runtime detail
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = file.metadata().await?.permissions();
        perms.set_mode(0o600);
        file.set_permissions(perms).await?;
    }

    file.flush().await?;
    tokio::fs::rename(tmp_path, path).await?;
    Ok(())
}

/// Load persisted event history. Event history is auxiliary data: a missing or
/// corrupt file never blocks startup — it degrades to an empty map with a warning.
pub async fn load_events_with_recovery(
    path: &Path,
) -> anyhow::Result<HashMap<Uuid, Vec<ProgramEventRecord>>> {
    match load_events_internal(path).await {
        Ok(data) => Ok(data),
        Err(e) => {
            tracing::error!("Failed to load event history {:?}: {}", path, e);
            // Fall back to backup if present
            let backup_path = path.with_extension("json.bak");
            if let Ok(data) = load_events_internal(&backup_path).await {
                tracing::warn!("Recovered event history from backup: {:?}", backup_path);
                return Ok(data);
            }
            tracing::warn!("Starting with empty event history");
            Ok(HashMap::new())
        }
    }
}

async fn load_events_internal(
    path: &Path,
) -> anyhow::Result<HashMap<Uuid, Vec<ProgramEventRecord>>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = tokio::fs::read_to_string(path).await?;
    if content.trim().is_empty() {
        return Ok(HashMap::new());
    }
    Ok(serde_json::from_str(&content)?)
}
