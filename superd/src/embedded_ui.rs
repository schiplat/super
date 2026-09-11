//! Built-in Dashboard assets (OSS). Served when no licensed `ui` plugin is loaded.

use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../dashboard/dist"]
struct EmbeddedAssets;

pub fn resolve(file_path: &str) -> Option<(String, Bytes)> {
    let path = if file_path.is_empty() || file_path == "/" {
        "index.html"
    } else {
        file_path.trim_start_matches('/')
    };

    let file = EmbeddedAssets::get(path)?;
    let mime = mime_for(path).to_string();
    Some((mime, Bytes::copy_from_slice(file.data.as_ref())))
}

pub fn response(
    file_path: &str,
    auth_required: bool,
    is_licensed: bool,
    inject_config: impl FnOnce(&[u8], bool, bool) -> Bytes,
) -> Option<Response> {
    let (mime, data) = resolve(file_path)?;
    let body = if inject_needed(file_path) {
        inject_config(&data, auth_required, is_licensed)
    } else {
        data
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&mime)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    // index.html must not stick in the browser after deploys (hashed assets are immutable).
    if inject_needed(file_path) {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    } else {
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    }
    Some((headers, body).into_response())
}

pub fn spa_index(
    auth_required: bool,
    is_licensed: bool,
    inject_config: impl FnOnce(&[u8], bool, bool) -> Bytes,
) -> Response {
    response("index.html", auth_required, is_licensed, inject_config)
        .unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}

fn inject_needed(file_path: &str) -> bool {
    file_path.is_empty()
        || file_path == "/"
        || file_path == "index.html"
        || file_path.ends_with("/index.html")
}

fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html",
        Some("js") | Some("mjs") => "application/javascript",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("webmanifest") => "application/manifest+json",
        Some("map") => "application/json",
        _ => "application/octet-stream",
    }
}
