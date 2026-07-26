//! Generic HTTP bridge for optional `super_plugin_http_v1` exports.

use crate::SystemPaths;
use crate::plugin::loader::PluginRuntime;
use axum::{
    Json, Router,
    extract::{ConnectInfo, State},
    http::{Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::MethodRouter,
};
use common::UserContext;
use common::plugin_http_abi::{HTTP_PLUGIN_API_VERSION, HTTP_PLUGIN_SYMBOL, SuperPluginHttpV1};
use libloading::Library;
use serde::Deserialize;
use std::collections::HashMap;
use std::ffi::CString;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::warn;

#[derive(Clone)]
struct HttpPluginHandle {
    init: Option<common::plugin_http_abi::HttpInitFn>,
    authenticate: Option<common::plugin_http_abi::HttpAuthFn>,
    authorize: Option<common::plugin_http_abi::HttpRbacFn>,
    audit_request: Option<common::plugin_http_abi::HttpAuditFn>,
    handle_api: common::plugin_http_abi::HttpApiFn,
}

#[derive(Clone)]
struct PluginAuthContext {
    ctx_json: String,
}

#[derive(Debug, Deserialize)]
struct RouteSpec {
    method: String,
    path: String,
}

impl HttpPluginHandle {
    fn from_vtable(vtable: SuperPluginHttpV1) -> anyhow::Result<Self> {
        if vtable.api_version != HTTP_PLUGIN_API_VERSION {
            anyhow::bail!(
                "HTTP plugin API version {} != host {}",
                vtable.api_version,
                HTTP_PLUGIN_API_VERSION
            );
        }
        Ok(Self {
            init: vtable.init,
            authenticate: vtable.authenticate,
            authorize: vtable.authorize,
            audit_request: vtable.audit_request,
            handle_api: vtable
                .handle_api
                .ok_or_else(|| anyhow::anyhow!("HTTP plugin missing handle_api"))?,
        })
    }

    fn init(&self, config_json: &str) -> anyhow::Result<()> {
        let Some(init) = self.init else {
            return Ok(());
        };
        let cstr = CString::new(config_json)?;
        // SAFETY: `cstr` is a valid NUL-terminated `CString` that outlives the
        // call; `init` is a checked vtable entry point (api_version verified).
        let code = unsafe { init(cstr.as_ptr()) };
        if code != 0 {
            anyhow::bail!("HTTP plugin init failed ({code})");
        }
        Ok(())
    }

    fn call_api(&self, method: &str, path: &str, body: &str, ctx_json: &str) -> (u16, String) {
        let method_c = CString::new(method).unwrap_or_default();
        let path_c = CString::new(path).unwrap_or_default();
        let body_c = CString::new(body).unwrap_or_default();
        let ctx_c = CString::new(ctx_json).unwrap_or_default();
        let mut buf = vec![0u8; 65536];
        // SAFETY: the four input pointers are valid NUL-terminated `CString`s
        // and `buf` is a valid writable buffer of `buf.len()` bytes, all
        // outliving the call; the ABI contract is that the plugin writes a
        // NUL-terminated response of at most `buf.len()` bytes.
        let status = unsafe {
            (self.handle_api)(
                method_c.as_ptr(),
                path_c.as_ptr(),
                body_c.as_ptr(),
                ctx_c.as_ptr(),
                buf.as_mut_ptr().cast(),
                buf.len(),
            )
        };
        let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        (
            status as u16,
            String::from_utf8_lossy(&buf[..nul]).into_owned(),
        )
    }
}

fn is_core_reserved_route(method: &str, path: &str) -> bool {
    matches!(
        (method.to_uppercase().as_str(), path),
        ("GET", "/api/v1/system/license")
    )
}

fn load_http_vtable(library: &Library) -> Option<SuperPluginHttpV1> {
    // SAFETY: symbol must match `SuperPluginHttpV1` ABI when present.
    unsafe {
        let symbol: Result<libloading::Symbol<unsafe extern "C" fn() -> SuperPluginHttpV1>, _> =
            library.get(HTTP_PLUGIN_SYMBOL);
        symbol.ok().map(|s| s())
    }
}

fn list_routes(vtable: &SuperPluginHttpV1) -> anyhow::Result<Vec<RouteSpec>> {
    let Some(list_routes) = vtable.list_routes else {
        anyhow::bail!("HTTP plugin missing list_routes");
    };
    let mut buf = vec![0u8; 8192];
    // SAFETY: `buf` is a valid writable buffer of `buf.len()` bytes for the
    // duration of the call; the ABI contract is that the plugin writes a
    // NUL-terminated route list of at most `buf.len()` bytes and returns the
    // number of bytes written.
    let n = unsafe { list_routes(buf.as_mut_ptr().cast(), buf.len()) } as usize;
    if n == 0 || n >= buf.len() {
        anyhow::bail!("HTTP plugin list_routes returned empty buffer");
    }
    let nul = buf[..n].iter().position(|&b| b == 0).unwrap_or(n);
    let json = std::str::from_utf8(&buf[..nul])?;
    Ok(serde_json::from_str(json)?)
}

/// Merge plugin HTTP routes and apply auth middleware when a plugin provides it.
///
/// Returns the merged router and whether API authentication is active.
pub fn attach_http_plugins(
    router: Router,
    runtime: &PluginRuntime,
    paths: &SystemPaths,
) -> anyhow::Result<(Router, bool)> {
    let loaded_plugins: Vec<_> = runtime
        .plugin_versions
        .iter()
        .map(|(id, version)| {
            serde_json::json!({
                "id": id,
                "version": version,
            })
        })
        .collect();
    let init_json = serde_json::json!({
        "super_root": paths.root,
        "loaded_plugins": loaded_plugins,
    });
    let init_payload = init_json.to_string();

    let mut plugin_routes: Vec<(RouteSpec, Arc<HttpPluginHandle>)> = Vec::new();
    let mut auth_plugin: Option<Arc<HttpPluginHandle>> = None;

    for plugin_id in &runtime.loaded_ids {
        let Some(library) = runtime.library(plugin_id) else {
            continue;
        };
        let Some(vtable) = load_http_vtable(library) else {
            continue;
        };

        let routes = list_routes(&vtable)?;
        let handle = Arc::new(HttpPluginHandle::from_vtable(vtable)?);
        handle.init(&init_payload)?;

        if handle.authenticate.is_some() && auth_plugin.is_none() {
            auth_plugin = Some(handle.clone());
        }

        for route in routes {
            if is_core_reserved_route(&route.method, &route.path) {
                warn!(
                    "Skipping plugin route {} {} (handled by super-core)",
                    route.method, route.path
                );
                continue;
            }
            plugin_routes.push((route, handle.clone()));
        }
    }

    if plugin_routes.is_empty() {
        return Ok((router, false));
    }

    let mut grouped: HashMap<String, (Arc<HttpPluginHandle>, Vec<String>)> = HashMap::new();
    for (route, handle) in plugin_routes {
        let entry = grouped
            .entry(route.path.clone())
            .or_insert_with(|| (handle, Vec::new()));
        entry.1.push(route.method.to_uppercase());
    }

    let mut plugin_router = Router::new();
    for (path, (handle, methods)) in grouped {
        let mut method_router = MethodRouter::new();
        for method in methods {
            let handler = plugin_api_handler;
            method_router = match method.as_str() {
                "GET" => method_router.get(handler),
                "POST" => method_router.post(handler),
                "PUT" => method_router.put(handler),
                "DELETE" => method_router.delete(handler),
                "PATCH" => method_router.patch(handler),
                other => {
                    warn!("Unsupported HTTP method '{}' for {}", other, path);
                    method_router
                }
            };
        }
        plugin_router = plugin_router.route(&path, method_router.with_state(handle));
    }

    // Core routes registered first; plugins only add non-conflicting paths.
    let mut router = router.merge(plugin_router);

    if let Some(handle) = auth_plugin.clone() {
        router = router
            .layer(axum::middleware::from_fn_with_state(
                handle.clone(),
                plugin_rbac_middleware,
            ))
            .layer(axum::middleware::from_fn_with_state(
                handle.clone(),
                plugin_audit_middleware,
            ))
            .layer(axum::middleware::from_fn_with_state(
                handle,
                plugin_auth_middleware,
            ));
    }

    Ok((router, auth_plugin.is_some()))
}

async fn plugin_api_handler(
    State(handle): State<Arc<HttpPluginHandle>>,
    req: axum::extract::Request,
) -> Response {
    let method = req.method().as_str().to_string();
    let path = req.uri().path().to_string();
    let ctx_json = req
        .extensions()
        .get::<PluginAuthContext>()
        .map(|c| c.ctx_json.clone())
        .unwrap_or_default();
    let body = axum::body::to_bytes(req.into_body(), 1024 * 1024)
        .await
        .unwrap_or_default();
    let body_str = String::from_utf8_lossy(&body);
    let (status, resp_body) = handle.call_api(&method, &path, &body_str, &ctx_json);
    api_response(status, resp_body)
}

fn api_response(status: u16, body: String) -> Response {
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    if body.is_empty() {
        return code.into_response();
    }
    let mut resp = Response::new(axum::body::Body::from(body));
    *resp.status_mut() = code;
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    resp
}

async fn plugin_auth_middleware(
    State(handle): State<Arc<HttpPluginHandle>>,
    mut req: axum::extract::Request,
    next: Next,
) -> Response {
    let Some(authenticate) = handle.authenticate else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let path = req.uri().path();
    let auth = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let query = req.uri().query().unwrap_or("");

    let Ok(path_c) = CString::new(path) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let auth_c = CString::new(auth).unwrap_or_default();
    let query_c = CString::new(query).unwrap_or_default();
    let mut buf = vec![0u8; 2048];

    let code = unsafe {
        // SAFETY: the three input pointers are valid NUL-terminated `CString`s
        // and `buf` is a valid writable buffer of `buf.len()` bytes, all
        // outliving the call; `authenticate` is a checked vtable entry point.
        authenticate(
            path_c.as_ptr(),
            auth_c.as_ptr(),
            query_c.as_ptr(),
            buf.as_mut_ptr().cast(),
            buf.len(),
        )
    };

    match code {
        3 => next.run(req).await,
        0 => {
            let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            let Ok(json) = std::str::from_utf8(&buf[..nul]) else {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            };
            req.extensions_mut().insert(PluginAuthContext {
                ctx_json: json.to_string(),
            });
            if let Ok(user) = serde_json::from_str::<UserContext>(json) {
                req.extensions_mut().insert(user);
            }
            next.run(req).await
        }
        1 => {
            warn!("Unauthorized access to {}", path);
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "status": "error",
                    "message": "unauthorized",
                })),
            )
                .into_response()
        }
        4 => {
            let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            let detail = std::str::from_utf8(&buf[..nul]).unwrap_or("");
            let body = if detail.trim().starts_with('{') {
                serde_json::from_str::<serde_json::Value>(detail).unwrap_or_else(|_| {
                    serde_json::json!({
                        "status": "error",
                        "message": "unauthorized",
                    })
                })
            } else {
                serde_json::json!({
                    "status": "error",
                    "message": if detail.is_empty() {
                        "unauthorized"
                    } else {
                        detail
                    },
                })
            };
            warn!("Unauthorized access to {} ({})", path, body);
            (StatusCode::UNAUTHORIZED, Json(body)).into_response()
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn plugin_rbac_middleware(
    State(handle): State<Arc<HttpPluginHandle>>,
    req: axum::extract::Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let authorize = handle.authorize.ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let Some(ctx) = req.extensions().get::<PluginAuthContext>() else {
        return Ok(next.run(req).await);
    };

    let path = req.uri().path();
    let method = req.method().as_str();
    let path_c = CString::new(path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let method_c = CString::new(method).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let ctx_c = CString::new(ctx.ctx_json.as_str()).unwrap_or_default();

    // SAFETY: all three pointers are valid NUL-terminated `CString`s that
    // outlive the call; `authorize` is a checked vtable entry point.
    let code = unsafe { authorize(path_c.as_ptr(), method_c.as_ptr(), ctx_c.as_ptr()) };
    if code == 0 {
        Ok(next.run(req).await)
    } else {
        warn!("Forbidden access to {}", path);
        Err(StatusCode::FORBIDDEN)
    }
}

async fn plugin_audit_middleware(
    State(handle): State<Arc<HttpPluginHandle>>,
    req: axum::extract::Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let audit_request = handle
        .audit_request
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let method = req.method().clone();
    if method == Method::GET || method == Method::OPTIONS || method == Method::HEAD {
        return Ok(next.run(req).await);
    }

    let path = req.uri().path().to_string();
    let ctx_json = req
        .extensions()
        .get::<PluginAuthContext>()
        .map(|c| c.ctx_json.clone())
        .unwrap_or_default();
    let ip = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip().to_string())
        .unwrap_or_else(|| "-".to_string());

    let response = next.run(req).await;
    let status = response.status().as_u16();

    let ctx_c = CString::new(ctx_json.as_str()).unwrap_or_default();
    let method_c = CString::new(method.as_str()).unwrap_or_default();
    let path_c = CString::new(path.as_str()).unwrap_or_default();
    let ip_c = CString::new(ip).unwrap_or_default();

    // SAFETY: all four pointers are valid NUL-terminated `CString`s that
    // outlive the call; `audit_request` is a checked vtable entry point.
    unsafe {
        audit_request(
            ctx_c.as_ptr(),
            method_c.as_ptr(),
            path_c.as_ptr(),
            status,
            ip_c.as_ptr(),
        );
    }

    Ok(response)
}
