//! Instance branding: a user-uploaded logo that replaces the bundled
//! dashboard logo in the sidebar.
//!
//! Storage is a plain file pair under the instance data directory
//! (`branding/logo.<ext>` + `branding/logo.meta.json`) — branding is purely
//! cosmetic, so losing it just means the dashboard falls back to the bundled
//! logo until it is re-uploaded; it does not belong in `auth.db`.
//!
//! `GET /api/v1/branding/logo` is auth-exempt: the sidebar renders it with a
//! plain `<img>`, which cannot carry a Bearer header. PUT/DELETE are Admin.

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use common::UserContext;
use serde_json::json;
use std::path::{Path, PathBuf};

use crate::api::{AppError, AppState};

pub const MAX_LOGO_BYTES: usize = 1024 * 1024;
const META_FILE: &str = "logo.meta.json";

pub fn branding_routes() -> Router<AppState> {
    Router::new().route(
        "/api/v1/branding/logo",
        axum::routing::get(get_logo)
            .put(put_logo)
            .delete(delete_logo),
    )
}

/// (extension, content type) for an uploaded image, judged by magic bytes.
/// SVG is detected as text: the first kilobyte must contain an `<svg` tag
/// (after an optional XML declaration). Anything else is rejected.
pub fn sniff_image(bytes: &[u8]) -> Result<(&'static str, &'static str), String> {
    if bytes.is_empty() {
        return Err("the logo file is empty".into());
    }
    if bytes.starts_with(b"\x89PNG") {
        return Ok(("png", "image/png"));
    }
    if bytes.starts_with(b"\xff\xd8\xff") {
        return Ok(("jpg", "image/jpeg"));
    }
    if bytes.starts_with(b"GIF8") {
        return Ok(("gif", "image/gif"));
    }
    if bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Ok(("webp", "image/webp"));
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]);
    let trimmed = head.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with("<svg") || (trimmed.starts_with("<?xml") && head.contains("<svg")) {
        return Ok(("svg", "image/svg+xml"));
    }
    Err("unsupported image type — use SVG, PNG, JPEG, or WebP".into())
}

fn meta_path(dir: &Path) -> PathBuf {
    dir.join(META_FILE)
}

/// Persist the logo + its meta. Returns (file name, content type).
pub fn store_logo(
    dir: &Path,
    bytes: &[u8],
    uploaded_by: &str,
) -> Result<(String, &'static str), String> {
    if bytes.len() > MAX_LOGO_BYTES {
        return Err("the logo file is too large — the limit is 1 MiB".into());
    }
    let (ext, content_type) = sniff_image(bytes)?;
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("cannot create the branding directory: {e}"))?;
    // Remove any previous logo first: the extension may change.
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("logo.") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    let file = format!("logo.{ext}");
    std::fs::write(dir.join(&file), bytes)
        .map_err(|e| format!("cannot write the logo file: {e}"))?;
    let meta = json!({
        "file": file,
        "content_type": content_type,
        "uploaded_at": now_secs(),
        "uploaded_by": uploaded_by,
    });
    std::fs::write(
        meta_path(dir),
        serde_json::to_vec(&meta).unwrap_or_default(),
    )
    .map_err(|e| format!("cannot write the logo metadata: {e}"))?;
    Ok((file, content_type))
}

/// (file path, content type) when a custom logo is stored.
pub fn read_logo(dir: &Path) -> Option<(PathBuf, String)> {
    let meta = std::fs::read(meta_path(dir)).ok()?;
    let meta: serde_json::Value = serde_json::from_slice(&meta).ok()?;
    let file = meta.get("file")?.as_str()?.to_string();
    let content_type = meta.get("content_type")?.as_str()?.to_string();
    let path = dir.join(file);
    path.exists().then_some((path, content_type))
}

pub fn clear_logo(dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name == META_FILE || name.to_string_lossy().starts_with("logo.") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

async fn get_logo(State(state): State<AppState>) -> Response {
    match read_logo(&state.branding_dir) {
        Some((path, content_type)) => match std::fs::read(&path) {
            Ok(bytes) => (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, content_type),
                    // Tiny local file; skip caching so an upload shows up
                    // without a cache-busting dance.
                    (header::CACHE_CONTROL, "no-store".to_string()),
                ],
                bytes,
            )
                .into_response(),
            Err(_) => logo_not_found(),
        },
        None => logo_not_found(),
    }
}

async fn put_logo(
    State(state): State<AppState>,
    axum::Extension(user): axum::Extension<UserContext>,
    bytes: axum::body::Bytes,
) -> Result<Response, AppError> {
    let dir = state.branding_dir.clone();
    let uploaded_by = user.name.clone();
    let dir_for_log = dir.display().to_string();
    let (file, content_type) = store_logo(&dir, &bytes, &uploaded_by)
        .map_err(|e| AppError(StatusCode::BAD_REQUEST, anyhow::anyhow!(e)))?;
    tracing::info!(
        file = %file,
        content_type = %content_type,
        bytes = bytes.len(),
        dir = %dir_for_log,
        by = %user.name,
        "custom dashboard logo stored"
    );
    Ok(Json(json!({
        "content_type": content_type,
        "bytes": bytes.len(),
    }))
    .into_response())
}

async fn delete_logo(State(state): State<AppState>) -> Response {
    clear_logo(&state.branding_dir);
    StatusCode::NO_CONTENT.into_response()
}

fn logo_not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "status": "error",
            "message": "no custom logo is set",
        })),
    )
        .into_response()
}
