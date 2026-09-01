use common::{ProcessStatus, ProgramInfo, ProgramSummary, WsMessage};
use futures_util::StreamExt;
use http_body_util::Full as FullBody;
use hyper::Request as HyperRequest;
use reqwest::header;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio_tungstenite::connect_async;
use url::Url;
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub enum WaitTarget {
    Up,      // Running/Healthy
    Healthy, // Healthy only (readiness check passed)
    Down,    // Stopped

    // Restart-only: carries the previous PID
    // None if stopped before restart; Some holds the previous PID
    Restarted(Option<u32>),
}

/// Split a server string into (http_base_url, optional Unix socket path).
///
/// `unix:///run/superd.sock` → `("http://localhost", Some("/run/superd.sock"))`.
/// A relative `unix://run/superd.sock` resolves under `SUPER_ROOT` (else cwd).
/// Any other value is treated as an http(s) URL, trimmed of trailing slashes.
pub fn split_server(server: &str) -> (String, Option<PathBuf>) {
    let server = server.trim();
    if let Some(rest) = server.strip_prefix("unix://") {
        let socket = PathBuf::from(rest);
        let resolved = if socket.is_absolute() {
            socket
        } else {
            std::env::var("SUPER_ROOT")
                .map(|root| PathBuf::from(root).join(&socket))
                .unwrap_or(socket)
        };
        ("http://localhost".to_string(), Some(resolved))
    } else {
        (server.trim_end_matches('/').to_string(), None)
    }
}

/// Best-effort discovery of the daemon's default Unix socket endpoint.
///
/// Mirrors how the daemon resolves a relative `[server] socket` value:
/// `$SUPER_ROOT/run/superd.sock` (else `<cwd>/run/superd.sock`). Returns
/// `Some` only when that path exists and is a real socket file — a plain
/// file or symlink is ignored so a swapped/hijacked path can't silently
/// redirect the CLI to a different transport.
#[cfg(unix)]
pub fn discover_default_socket() -> Option<PathBuf> {
    use std::os::unix::fs::FileTypeExt;
    let candidate = std::env::var("SUPER_ROOT")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("run/superd.sock");
    let meta = std::fs::symlink_metadata(&candidate).ok()?;
    meta.file_type().is_socket().then_some(candidate)
}

#[cfg(not(unix))]
pub fn discover_default_socket() -> Option<PathBuf> {
    None
}

/// HTTP client that talks to the API over TCP (`http://`) or a Unix domain
/// socket (`unix://`). All request paths are built against `http://localhost`
/// in unix mode; the socket path is carried here and the host is ignored.
#[derive(Clone)]
pub enum ApiClient {
    Http(reqwest::Client),
    Unix(Box<UnixHttpClient>),
}

#[derive(Clone)]
pub struct UnixHttpClient {
    client: hyper_util::client::legacy::Client<hyperlocal::UnixConnector, FullBody<bytes::Bytes>>,
    socket: PathBuf,
    token: Option<String>,
}

impl ApiClient {
    pub fn get(&self, url: impl AsRef<str>) -> ApiRequest<'_> {
        ApiRequest {
            client: self,
            method: reqwest::Method::GET,
            url: url.as_ref().to_string(),
            body: None,
        }
    }
    pub fn post(&self, url: impl AsRef<str>) -> ApiRequest<'_> {
        ApiRequest {
            client: self,
            method: reqwest::Method::POST,
            url: url.as_ref().to_string(),
            body: None,
        }
    }
    pub fn put(&self, url: impl AsRef<str>) -> ApiRequest<'_> {
        ApiRequest {
            client: self,
            method: reqwest::Method::PUT,
            url: url.as_ref().to_string(),
            body: None,
        }
    }
    pub fn delete(&self, url: impl AsRef<str>) -> ApiRequest<'_> {
        ApiRequest {
            client: self,
            method: reqwest::Method::DELETE,
            url: url.as_ref().to_string(),
            body: None,
        }
    }
}

/// Response envelope covering both transports. Handlers only touch `status()`,
/// `json()` and `text()`, so they stay transport-agnostic.
pub enum ApiResponse {
    Http(reqwest::Response),
    Unix {
        status: reqwest::StatusCode,
        body: bytes::Bytes,
    },
}

impl ApiResponse {
    pub fn status(&self) -> reqwest::StatusCode {
        match self {
            ApiResponse::Http(r) => r.status(),
            ApiResponse::Unix { status, .. } => *status,
        }
    }
    pub async fn json<T: serde::de::DeserializeOwned>(self) -> anyhow::Result<T> {
        match self {
            ApiResponse::Http(r) => Ok(r.json().await?),
            ApiResponse::Unix { body, .. } => Ok(serde_json::from_slice(&body)?),
        }
    }
    pub async fn text(self) -> anyhow::Result<String> {
        match self {
            ApiResponse::Http(r) => Ok(r.text().await?),
            ApiResponse::Unix { body, .. } => Ok(String::from_utf8_lossy(&body).into_owned()),
        }
    }
}

/// Request builder mirroring the `reqwest` chain shape (`get/post/put/delete`
/// then `.json(&body).send()`), producing an [`ApiResponse`] on either
/// transport so all handler code stays transport-agnostic.
pub struct ApiRequest<'a> {
    client: &'a ApiClient,
    method: reqwest::Method,
    url: String,
    body: Option<bytes::Bytes>,
}

impl<'a> ApiRequest<'a> {
    pub fn json(mut self, value: &impl serde::Serialize) -> Self {
        self.body = serde_json::to_vec(value).ok().map(bytes::Bytes::from);
        self
    }

    pub async fn send(self) -> anyhow::Result<ApiResponse> {
        match self.client {
            ApiClient::Http(c) => {
                let mut rb = c.request(self.method.clone(), &self.url);
                if let Some(body) = &self.body {
                    rb = rb
                        .body(body.clone())
                        .header(header::CONTENT_TYPE, "application/json");
                }
                Ok(ApiResponse::Http(rb.send().await?))
            }
            ApiClient::Unix(u) => u.send(self.method, self.url, self.body).await,
        }
    }
}

impl UnixHttpClient {
    async fn send(
        &self,
        method: reqwest::Method,
        url: String,
        body: Option<bytes::Bytes>,
    ) -> anyhow::Result<ApiResponse> {
        use http_body_util::BodyExt;

        let parsed: Url = url.parse()?;
        let path = parsed.path().to_string();
        let path_and_query = match parsed.query() {
            Some(q) => format!("{path}?{q}"),
            None => path,
        };
        let unix_uri = hyperlocal::Uri::new(&self.socket, &path_and_query);

        let mut builder = HyperRequest::builder()
            .method(method.as_str())
            .uri(unix_uri);
        if let Some(token) = &self.token {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let req = match body {
            Some(b) => builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(FullBody::from(b))?,
            None => builder.body(FullBody::new(bytes::Bytes::new()))?,
        };
        let resp = self.client.request(req).await?;
        let (parts, resp_body) = resp.into_parts();
        let body = resp_body.collect().await?.to_bytes();
        Ok(ApiResponse::Unix {
            status: parts.status,
            body,
        })
    }
}

/// Build an HTTP client with optional Bearer token.
pub fn build_client(token: Option<&String>) -> anyhow::Result<reqwest::Client> {
    let mut headers = header::HeaderMap::new();
    if let Some(t) = token {
        let mut auth_val = header::HeaderValue::from_str(&format!("Bearer {t}"))?;
        auth_val.set_sensitive(true);
        headers.insert(header::AUTHORIZATION, auth_val);
    }
    let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()?;
    Ok(client)
}

/// Build an API client for a `--server` / config value (`http(s)://` or `unix://`).
pub fn build_api_client(server: &str, token: Option<&String>) -> anyhow::Result<ApiClient> {
    let (_, socket) = split_server(server);
    if let Some(sock) = socket {
        if sock.as_os_str().is_empty() {
            anyhow::bail!("unix:// server URL has an empty socket path");
        }
        Ok(ApiClient::Unix(Box::new(UnixHttpClient {
            client: hyper_util::client::legacy::Client::builder(
                hyper_util::rt::TokioExecutor::new(),
            )
            .build(hyperlocal::UnixConnector),
            socket: sock,
            token: token.cloned(),
        })))
    } else {
        Ok(ApiClient::Http(build_client(token)?))
    }
}

/// Verify credentials against a server with the security plugin (POST /api/v1/auth/login).
pub async fn verify_credentials(server: &str, token: &str) -> anyhow::Result<()> {
    let client = build_api_client(server, Some(&token.to_string()))?;
    let (base, _) = split_server(server);
    let url = format!("{}/api/v1/auth/login", base.trim_end_matches('/'));
    let resp = client.post(&url).send().await?;

    match resp.status() {
        reqwest::StatusCode::OK => Ok(()),
        reqwest::StatusCode::UNAUTHORIZED => {
            let body = resp.text().await.unwrap_or_default();
            let detail = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| {
                    v.get("message")
                        .and_then(|m| m.as_str())
                        .map(str::to_string)
                })
                .filter(|m| !m.is_empty() && m != "unauthorized");
            if let Some(msg) = detail {
                Err(anyhow::anyhow!("Login failed: {msg}"))
            } else {
                Err(anyhow::anyhow!("Login failed: invalid token."))
            }
        }
        reqwest::StatusCode::NOT_FOUND => Err(anyhow::anyhow!(
            "Login requires superd with the security plugin loaded. \
             This server ({server}) returned 404 for /api/v1/auth/login."
        )),
        status => Err(anyhow::anyhow!("Login failed: server returned {status}.")),
    }
}

/// Best-effort server logout (ends sticky root `auth_secret` session when applicable).
pub async fn server_logout(server: &str, token: &str) -> anyhow::Result<()> {
    let client = build_api_client(server, Some(&token.to_string()))?;
    let (base, _) = split_server(server);
    let url = format!("{}/api/v1/auth/logout", base.trim_end_matches('/'));
    let resp = client.post(&url).send().await?;
    if resp.status().is_success() || resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Ok(());
    }
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(());
    }
    Err(anyhow::anyhow!(
        "Logout request failed: server returned {}.",
        resp.status()
    ))
}

/// Resolve target to program IDs. `target` is `all`, `@group`, or a name/id.
pub async fn resolve_targets(
    client: &ApiClient,
    base_url: &str,
    target: &str,
) -> anyhow::Result<Vec<Uuid>> {
    Ok(resolve_target_details(client, base_url, target)
        .await?
        .into_iter()
        .map(|p| p.id)
        .collect())
}

/// Resolve target to full `ProgramSummary` details (IDs + names + groups),
/// mirroring [`resolve_targets`] filtering but keeping names for previews.
pub async fn resolve_target_details(
    client: &ApiClient,
    base_url: &str,
    target: &str,
) -> anyhow::Result<Vec<ProgramSummary>> {
    let url = format!("{}/api/v1/programs", base_url);
    let resp = client.get(&url).send().await?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(anyhow::anyhow!(
            "Error: Unauthorized. Run `super login`, or pass --token / set SUPER_TOKEN."
        ));
    }
    let programs: Vec<ProgramSummary> = resp.json().await?;

    if target == "all" {
        if programs.is_empty() {
            return Err(anyhow::anyhow!("No programs found on server."));
        }
        return Ok(programs);
    }

    if let Some(group_name) = target.strip_prefix('@') {
        let matched: Vec<ProgramSummary> = programs
            .into_iter()
            .filter(|p| p.group.as_deref() == Some(group_name))
            .collect();
        if matched.is_empty() {
            return Err(anyhow::anyhow!(
                "Error: No programs found in group '@{}'",
                group_name
            ));
        }
        return Ok(matched);
    }

    let matches: Vec<_> = programs
        .into_iter()
        .filter(|p| p.name == target || p.id.to_string().starts_with(target))
        .collect();

    match matches.len() {
        0 => Err(anyhow::anyhow!("Error: Program not found: '{}'", target)),
        1 => Ok(matches),
        _ => {
            eprintln!(
                "Error: Ambiguous target '{}'. Found multiple matches:",
                target
            );
            for p in &matches {
                eprintln!("   {} ({})", p.id, p.name);
            }
            Err(anyhow::anyhow!("Please be more specific."))
        }
    }
}

/// Poll until target status is reached
pub async fn wait_for_status(
    client: &ApiClient,
    base_url: &str,
    id: Uuid,
    target: WaitTarget,
    timeout_sec: u64,
) -> anyhow::Result<()> {
    let start_time = Instant::now();
    let timeout = Duration::from_secs(timeout_sec);
    let url = format!("{}/api/v1/programs/{}", base_url, id);

    print!("   Verifying status...");
    let _ = std::io::stdout().flush();

    loop {
        if start_time.elapsed() > timeout {
            println!();
            return Err(anyhow::anyhow!(
                "Timeout: Status did not change within {}s.",
                timeout_sec
            ));
        }

        let resp = client.get(&url).send().await?;
        if !resp.status().is_success() {
            println!();
            return Err(anyhow::anyhow!("API Error during verification."));
        }
        let info: ProgramInfo = resp.json().await?;
        let current_state = info.state;
        let current_pid = info.pid;

        match target {
            WaitTarget::Up => {
                match current_state {
                    ProcessStatus::Running | ProcessStatus::Healthy => {
                        println!(" Confirmed (Running, PID: {:?}).", current_pid.unwrap_or(0));
                        return Ok(());
                    }
                    ProcessStatus::Fatal => {
                        println!(" Failed (Crashed/Fatal).");
                        return Err(anyhow::anyhow!("Process crashed immediately."));
                    }
                    ProcessStatus::Backoff => {
                        println!(" Unstable (Backoff).");
                        return Err(anyhow::anyhow!("Process is restarting (Backoff)."));
                    }
                    _ => {} // Waiting, Starting, etc.
                }
            }
            WaitTarget::Healthy => {
                match current_state {
                    ProcessStatus::Healthy => {
                        println!(" Confirmed (Healthy).");
                        return Ok(());
                    }
                    ProcessStatus::Fatal => {
                        println!(" Failed (Crashed/Fatal).");
                        return Err(anyhow::anyhow!("Process crashed before becoming healthy."));
                    }
                    ProcessStatus::Backoff => {
                        println!(" Unstable (Backoff).");
                        return Err(anyhow::anyhow!("Process is restarting (Backoff)."));
                    }
                    _ => {} // Waiting, Starting, Running (not yet healthy) etc.
                }
            }
            WaitTarget::Down => {
                if current_state == ProcessStatus::Stopped {
                    println!(" Confirmed (Stopped).");
                    return Ok(());
                }
            }
            WaitTarget::Restarted(old_pid) => {
                if matches!(current_state, ProcessStatus::Fatal | ProcessStatus::Backoff) {
                    println!(" Failed (Crashed during restart).");
                    return Err(anyhow::anyhow!("Process crashed during restart."));
                }

                if matches!(
                    current_state,
                    ProcessStatus::Running | ProcessStatus::Healthy
                ) {
                    match (old_pid, current_pid) {
                        (Some(old), Some(new)) if old != new => {
                            println!(" Confirmed (Restarted, PID: {} -> {}).", old, new);
                            return Ok(());
                        }
                        (None, Some(new)) => {
                            println!(" Confirmed (Started, PID: {}).", new);
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Stream logs over WebSocket. `base_url` is the http(s) base; when
/// `socket_path` is set the WebSocket handshake runs over the Unix socket.
pub async fn monitor_logs(
    base_url: &str,
    socket_path: Option<&Path>,
    id: Uuid,
    token_query: &str,
) -> anyhow::Result<()> {
    use futures_util::stream::Stream;
    use tokio_tungstenite::tungstenite::{Error as WsError, Message};

    let ws_base = base_url
        .replace("http://", "ws://")
        .replace("https://", "wss://");
    let ws_url = format!("{}/ws?id={}{}", ws_base, id, token_query);

    println!("Connecting to logs for {}...", id);
    let url = Url::parse(&ws_url)?;

    let (ws_stream, resp) = if let Some(sock) = socket_path {
        let stream = tokio::net::UnixStream::connect(sock).await?;
        let (s, r) = tokio_tungstenite::client_async(url.clone(), stream).await?;
        let stream: Box<dyn Stream<Item = Result<Message, WsError>> + Send + Unpin> = Box::new(s);
        (stream, r)
    } else {
        let (s, r) = connect_async(url).await?;
        let stream: Box<dyn Stream<Item = Result<Message, WsError>> + Send + Unpin> = Box::new(s);
        (stream, r)
    };
    let mut ws_stream = ws_stream;

    println!("Connected (Status: {})", resp.status());
    println!("---------------------------------------------------");

    while let Some(msg) = ws_stream.next().await {
        let msg = msg?;
        if msg.is_text() {
            let text = msg.to_text()?;
            if let Ok(ws_msg) = serde_json::from_str::<WsMessage>(text) {
                match ws_msg {
                    WsMessage::Log { source, line, .. } => {
                        let prefix = if source == "stderr" { "[ERR]" } else { "[OUT]" };
                        println!("{} {}", prefix, line);
                    }
                    WsMessage::StatusChange { status, .. } => {
                        println!("[SYS] Status changed to: {:?}", status);
                    }
                }
            }
        } else if msg.is_close() {
            println!("Server closed connection.");
            break;
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "super-cli-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn with_super_root(dir: &Path, f: impl FnOnce()) {
        // SAFETY: edition-2024 marks env mutation unsafe; these tests only
        // touch SUPER_ROOT and no other test in this crate reads it.
        unsafe { std::env::set_var("SUPER_ROOT", dir) };
        f();
        unsafe { std::env::remove_var("SUPER_ROOT") };
    }

    #[test]
    fn discover_ignores_missing_path() {
        let dir = temp_dir("missing");
        with_super_root(&dir, || assert_eq!(discover_default_socket(), None));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discover_ignores_plain_file_and_symlink() {
        let dir = temp_dir("plain");
        std::fs::create_dir_all(dir.join("run")).unwrap();
        std::fs::write(dir.join("run/superd.sock"), b"not a socket").unwrap();
        with_super_root(&dir, || assert_eq!(discover_default_socket(), None));

        std::fs::remove_file(dir.join("run/superd.sock")).unwrap();
        std::os::unix::fs::symlink("/tmp/nonexistent-target", dir.join("run/superd.sock")).unwrap();
        with_super_root(&dir, || assert_eq!(discover_default_socket(), None));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discover_accepts_real_socket() {
        let dir = temp_dir("real");
        std::fs::create_dir_all(dir.join("run")).unwrap();
        let sock_path = dir.join("run/superd.sock");
        // A bound UnixListener leaves a real socket file at the path. Some
        // sandboxes deny AF_UNIX bind entirely; skip rather than fail there.
        let listener = match std::os::unix::net::UnixListener::bind(&sock_path) {
            Ok(l) => l,
            Err(e)
                if e.kind() == std::io::ErrorKind::PermissionDenied
                    || e.raw_os_error() == Some(libc::EOPNOTSUPP) =>
            {
                let _ = std::fs::remove_dir_all(&dir);
                return;
            }
            Err(e) => panic!("bind unix socket: {e}"),
        };
        with_super_root(&dir, || {
            assert_eq!(discover_default_socket(), Some(sock_path.clone()));
        });
        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
