use common::ArtifactConfig;
use common::security::{FetchUrlPolicy, validate_outbound_url};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Soft caps for archive extraction (DoS / zip-bomb mitigation).
const MAX_EXTRACT_FILES: usize = 256;
const MAX_EXTRACT_BYTES: u64 = 512 * 1024 * 1024; // 512 MiB

/// Download phase (enhanced):
/// 1. Automatic retry with exponential backoff
/// 2. Fine-grained timeouts (connect vs transfer)
/// 3. Smart error handling (no retry on fatal 4xx)
/// 4. Optional safe archive extract (`artifact.extract`)
pub async fn download_to_staging(
    config: &ArtifactConfig,
    timeout_secs: u64,
) -> anyhow::Result<PathBuf> {
    validate_outbound_url(&config.source, FetchUrlPolicy::OtaArtifact)?;

    let target_path = PathBuf::from(&config.destination);

    // Ensure parent directory exists
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).await?;
    }

    let file_name = target_path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Invalid destination path"))?
        .to_string_lossy();
    // Download lands on `.download`; final swap staging is always `{name}.new`.
    let download_path = target_path.with_file_name(format!("{}.download", file_name));
    let staging_path = target_path.with_file_name(format!("{}.new", file_name));

    tracing::info!(
        "Downloading OTA update to {:?} (timeout={}s)",
        download_path,
        timeout_secs
    );

    // `timeout_secs == 0` → no overall transfer deadline (matches config docs).
    // Connect still fails closed after 10s so a black-holed peer cannot hang forever
    // before the first byte.
    let mut builder = reqwest::Client::builder().connect_timeout(Duration::from_secs(10));
    if timeout_secs > 0 {
        builder = builder.timeout(Duration::from_secs(timeout_secs));
    }
    let client = builder.build()?;

    let max_retries = 3;
    let mut attempt = 0;

    loop {
        match perform_download(&client, &config.source, &download_path).await {
            Ok(_) => break,
            Err(e) => {
                attempt += 1;

                let is_fatal = if let Some(status) = e
                    .downcast_ref::<reqwest::Error>()
                    .and_then(|re| re.status())
                {
                    status.is_client_error()
                } else {
                    false
                };

                if is_fatal || attempt > max_retries {
                    tracing::error!(
                        "Download failed permanently after {} attempts: {}",
                        attempt,
                        e
                    );
                    let _ = fs::remove_file(&download_path).await;
                    return Err(e);
                }

                let wait_secs = 2u64.pow(attempt as u32 - 1);
                tracing::warn!(
                    "Download failed: {}. Retrying in {}s (Attempt {}/{})",
                    e,
                    wait_secs,
                    attempt,
                    max_retries
                );
                tokio::time::sleep(Duration::from_secs(wait_secs)).await;
            }
        }
    }

    tracing::info!("Verifying checksum...");
    let calculated_hash = hash_file(&download_path).await?;
    if calculated_hash != config.checksum {
        let _ = fs::remove_file(&download_path).await;
        return Err(anyhow::anyhow!(
            "Checksum mismatch! Expected: {}, Got: {}",
            config.checksum,
            calculated_hash
        ));
    }

    if config.extract {
        tracing::info!("Extracting OTA archive to staging {:?}", staging_path);
        let dest_basename = file_name.to_string();
        let download_for_extract = download_path.clone();
        let staging_for_extract = staging_path.clone();
        tokio::task::spawn_blocking(move || {
            extract_archive_to_staging(&download_for_extract, &staging_for_extract, &dest_basename)
        })
        .await
        .map_err(|e| anyhow::anyhow!("extract task join error: {e}"))??;
        let _ = fs::remove_file(&download_path).await;
    } else {
        // Bare binary: promote download → `.new` staging.
        if staging_path.exists() {
            let _ = fs::remove_file(&staging_path).await;
        }
        fs::rename(&download_path, &staging_path).await?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&staging_path).await?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&staging_path, perms).await?;
    }

    Ok(staging_path)
}

async fn hash_file(path: &Path) -> anyhow::Result<String> {
    let mut file = fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];
    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

async fn perform_download(client: &reqwest::Client, url: &str, path: &Path) -> anyhow::Result<()> {
    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(response.error_for_status().unwrap_err()));
    }

    let mut file = fs::File::create(path).await?;
    let mut stream = response.bytes_stream();

    while let Some(item) = stream.next().await {
        let chunk = item?;
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    Ok(())
}

/// Unpack a verified archive and place the chosen payload at `staging_path`.
fn extract_archive_to_staging(
    archive_path: &Path,
    staging_path: &Path,
    dest_basename: &str,
) -> anyhow::Result<()> {
    let parent = staging_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid staging path"))?;
    let extract_dir = parent.join(format!(
        ".{}.extract",
        staging_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "ota".into())
    ));
    if extract_dir.exists() {
        std::fs::remove_dir_all(&extract_dir)?;
    }
    std::fs::create_dir_all(&extract_dir)?;

    let kind = detect_archive_kind(archive_path)?;
    match kind {
        ArchiveKind::TarGz => extract_tar(archive_path, &extract_dir, true)?,
        ArchiveKind::Tar => extract_tar(archive_path, &extract_dir, false)?,
        ArchiveKind::Zip => extract_zip(archive_path, &extract_dir)?,
    }

    let payload = pick_payload(&extract_dir, dest_basename)?;
    if staging_path.exists() {
        let _ = std::fs::remove_file(staging_path);
    }
    std::fs::rename(&payload, staging_path).or_else(|_| {
        std::fs::copy(&payload, staging_path)?;
        let _ = std::fs::remove_file(&payload);
        Ok::<(), std::io::Error>(())
    })?;
    let _ = std::fs::remove_dir_all(&extract_dir);
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    TarGz,
    Tar,
    Zip,
}

fn detect_archive_kind(path: &Path) -> anyhow::Result<ArchiveKind> {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        return Ok(ArchiveKind::TarGz);
    }
    if name.ends_with(".tar") {
        return Ok(ArchiveKind::Tar);
    }
    if name.ends_with(".zip") {
        return Ok(ArchiveKind::Zip);
    }

    // Magic-byte fallback when the download path has a generic suffix.
    let mut f = File::open(path)?;
    let mut magic = [0u8; 4];
    let n = f.read(&mut magic)?;
    if n >= 2 && magic[0] == 0x1f && magic[1] == 0x8b {
        return Ok(ArchiveKind::TarGz);
    }
    if n >= 4 && magic == *b"PK\x03\x04" {
        return Ok(ArchiveKind::Zip);
    }
    // ustar at offset 257 is expensive; treat unknown as error.
    anyhow::bail!(
        "Unsupported OTA archive format for {:?}; use .tar.gz, .tgz, .tar, or .zip",
        path.file_name()
    )
}

fn validate_member_path(name: &str) -> anyhow::Result<PathBuf> {
    let raw = name.trim_start_matches("./");
    if raw.is_empty() || raw.ends_with('/') {
        anyhow::bail!("archive member path rejected: {name:?}");
    }
    let path = Path::new(raw);
    if path.is_absolute() {
        anyhow::bail!("archive member must be relative: {name:?}");
    }
    for c in path.components() {
        match c {
            Component::Normal(_) => {}
            Component::CurDir => {}
            _ => anyhow::bail!("archive member path rejected: {name:?}"),
        }
    }
    Ok(path.to_path_buf())
}

fn extract_tar(archive_path: &Path, dest: &Path, gzip: bool) -> anyhow::Result<()> {
    let file = File::open(archive_path)?;
    if gzip {
        let decoder = flate2::read::GzDecoder::new(file);
        unpack_tar_entries(tar::Archive::new(decoder), dest)
    } else {
        unpack_tar_entries(tar::Archive::new(file), dest)
    }
}

fn unpack_tar_entries<R: Read>(mut archive: tar::Archive<R>, dest: &Path) -> anyhow::Result<()> {
    let mut files = 0usize;
    let mut bytes: u64 = 0;

    for entry in archive.entries()? {
        let mut entry = entry?;
        let header = entry.header().clone();
        let entry_type = header.entry_type();
        if entry_type.is_dir() {
            continue;
        }
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            anyhow::bail!("archive must not contain symlinks or hard links");
        }
        if !entry_type.is_file() {
            anyhow::bail!("unsupported archive entry type: {:?}", entry_type);
        }
        files += 1;
        if files > MAX_EXTRACT_FILES {
            anyhow::bail!("archive exceeds max file count ({MAX_EXTRACT_FILES})");
        }
        let name = entry.path()?.to_string_lossy().into_owned();
        let rel = validate_member_path(&name)?;
        let size = header.size()?;
        bytes = bytes.saturating_add(size);
        if bytes > MAX_EXTRACT_BYTES {
            anyhow::bail!("archive exceeds max uncompressed size ({MAX_EXTRACT_BYTES} bytes)");
        }
        let out_path = dest.join(&rel);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = File::create(&out_path)?;
        let mut limited = (&mut entry).take(size.saturating_add(1));
        let copied = std::io::copy(&mut limited, &mut out)?;
        if copied != size {
            anyhow::bail!("archive entry size mismatch for {name}");
        }
        out.flush()?;
    }
    Ok(())
}

fn extract_zip(archive_path: &Path, dest: &Path) -> anyhow::Result<()> {
    let file = File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut files = 0usize;
    let mut bytes: u64 = 0;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file
            .enclosed_name()
            .ok_or_else(|| anyhow::anyhow!("zip member path rejected: {:?}", file.name()))?
            .to_path_buf();
        if file.is_dir() {
            continue;
        }
        validate_member_path(&name.to_string_lossy())?;
        files += 1;
        if files > MAX_EXTRACT_FILES {
            anyhow::bail!("archive exceeds max file count ({MAX_EXTRACT_FILES})");
        }
        let size = file.size();
        bytes = bytes.saturating_add(size);
        if bytes > MAX_EXTRACT_BYTES {
            anyhow::bail!("archive exceeds max uncompressed size ({MAX_EXTRACT_BYTES} bytes)");
        }
        let out_path = dest.join(&name);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = File::create(&out_path)?;
        std::io::copy(&mut file, &mut out)?;
        out.flush()?;
    }
    Ok(())
}

fn pick_payload(extract_dir: &Path, dest_basename: &str) -> anyhow::Result<PathBuf> {
    let mut files = Vec::new();
    collect_regular_files(extract_dir, &mut files)?;
    if files.is_empty() {
        anyhow::bail!("archive contained no regular files");
    }

    let mut basename_matches: Vec<PathBuf> = files
        .iter()
        .filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy() == dest_basename)
                .unwrap_or(false)
        })
        .cloned()
        .collect();
    if basename_matches.len() == 1 {
        return Ok(basename_matches.remove(0));
    }
    if basename_matches.len() > 1 {
        anyhow::bail!(
            "archive contains multiple files named {dest_basename:?}; cannot choose payload"
        );
    }
    if files.len() == 1 {
        return Ok(files.remove(0));
    }
    anyhow::bail!(
        "archive has {} files and none named {dest_basename:?}; set destination basename to match or ship a single-file archive",
        files.len()
    )
}

fn collect_regular_files(dir: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            collect_regular_files(&path, out)?;
        } else if ft.is_file() {
            out.push(path);
        } else {
            anyhow::bail!("unexpected non-file entry after extract: {:?}", path);
        }
    }
    Ok(())
}

/// Backup phase: prefer hard link (fast, atomic); fall back to copy.
pub async fn create_backup(target: &Path) -> anyhow::Result<PathBuf> {
    let file_name = target
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Invalid path"))?;
    let backup = target.with_file_name(format!("{}.bak", file_name.to_string_lossy()));

    if target.exists() {
        if backup.exists() {
            let _ = fs::remove_file(&backup).await;
        }

        if fs::hard_link(target, &backup).await.is_err() {
            tracing::warn!("Hardlink failed, falling back to copy for backup.");
            fs::copy(target, &backup).await?;
        }
    }
    Ok(backup)
}

/// Apply update (atomic overwrite via rename).
pub async fn apply_update(target: &Path, staging: &Path) -> anyhow::Result<()> {
    if !staging.exists() {
        return Err(anyhow::anyhow!("Staging file missing"));
    }
    fs::rename(staging, target).await?;
    Ok(())
}

/// Rollback: restore backup to target path.
pub async fn rollback(target: &Path, backup: &Path) -> anyhow::Result<()> {
    if !backup.exists() {
        return Err(anyhow::anyhow!("Backup file missing, cannot rollback!"));
    }
    tracing::warn!("Rolling back binary from {:?} to {:?}", backup, target);
    fs::rename(backup, target).await?;
    Ok(())
}

/// Commit transaction: delete backup file.
pub async fn commit(backup: &Path) {
    if backup.exists() {
        let _ = fs::remove_file(backup).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn write_script(path: &Path, body: &str) {
        std::fs::write(path, body).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut p = std::fs::metadata(path).unwrap().permissions();
            p.set_mode(0o755);
            std::fs::set_permissions(path, p).unwrap();
        }
    }

    #[test]
    fn extract_tar_gz_single_file() {
        let dir = tempfile::tempdir().unwrap();
        let payload = dir.path().join("my-app");
        write_script(&payload, "#!/bin/sh\necho V2\n");
        let archive = dir.path().join("app.tar.gz");
        let status = Command::new("tar")
            .args(["-czf"])
            .arg(&archive)
            .arg("-C")
            .arg(dir.path())
            .arg("my-app")
            .status()
            .unwrap();
        assert!(status.success());

        let staging = dir.path().join("my-app.new");
        extract_archive_to_staging(&archive, &staging, "my-app").unwrap();
        assert!(staging.exists());
        assert!(std::fs::read_to_string(&staging).unwrap().contains("V2"));
    }

    #[test]
    fn extract_zip_basename_match_among_many() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("edge-agent");
        let extra = dir.path().join("README.txt");
        write_script(&bin, "#!/bin/sh\necho AGENT\n");
        std::fs::write(&extra, "notes").unwrap();
        let archive = dir.path().join("bundle.zip");
        let status = Command::new("zip")
            .args([
                "-j",
                archive.to_str().unwrap(),
                bin.to_str().unwrap(),
                extra.to_str().unwrap(),
            ])
            .status()
            .unwrap();
        assert!(status.success());

        let staging = dir.path().join("edge-agent.new");
        extract_archive_to_staging(&archive, &staging, "edge-agent").unwrap();
        assert!(std::fs::read_to_string(&staging).unwrap().contains("AGENT"));
    }

    #[test]
    fn extract_rejects_path_traversal_tar() {
        let dir = tempfile::tempdir().unwrap();
        // Craft a tar with a `../evil` member via Python for portability.
        let archive = dir.path().join("evil.tar");
        let py = r#"
import tarfile, sys
with tarfile.open(sys.argv[1], "w") as t:
    info = tarfile.TarInfo("../evil")
    data = b"x"
    info.size = len(data)
    import io
    t.addfile(info, io.BytesIO(data))
"#;
        let status = Command::new("python3")
            .args(["-c", py, archive.to_str().unwrap()])
            .status()
            .unwrap();
        assert!(status.success());
        let staging = dir.path().join("app.new");
        let err = extract_archive_to_staging(&archive, &staging, "app").unwrap_err();
        assert!(
            err.to_string().contains("rejected") || err.to_string().contains("relative"),
            "{err}"
        );
    }

    #[test]
    fn extract_multi_file_without_basename_errors() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        write_script(&a, "a");
        write_script(&b, "b");
        let archive = dir.path().join("multi.tar");
        let status = Command::new("tar")
            .args(["-cf"])
            .arg(&archive)
            .arg("-C")
            .arg(dir.path())
            .args(["a.bin", "b.bin"])
            .status()
            .unwrap();
        assert!(status.success());
        let staging = dir.path().join("app.new");
        let err = extract_archive_to_staging(&archive, &staging, "app").unwrap_err();
        assert!(err.to_string().contains("none named"), "{err}");
    }
}
