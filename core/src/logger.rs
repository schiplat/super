use chrono::{Local, Utc};
use common::WsMessage;
use common::config::{LogDriver, LogTimestamp};
use std::io::SeekFrom;
use std::path::Path;
use std::path::PathBuf;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::process::{ChildStderr, ChildStdout};
use tokio::sync::broadcast;
use uuid::Uuid;

// /// Production max line length (16KB)
// /// Prevents unbounded single-line output from causing OOM or WS disconnect
// const MAX_LINE_LENGTH: usize = 16 * 1024;

#[derive(Debug, Clone, Copy)]
pub enum LogSource {
    Stdout,
    Stderr,
}

impl LogSource {
    fn extension(&self) -> &'static str {
        match self {
            Self::Stdout => "out",
            Self::Stderr => "err",
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

#[derive(Clone)]
pub struct LogConfig {
    pub log_dir: PathBuf,
    pub max_size: u64, // bytes
    pub backups: u32,
    pub driver: LogDriver,
    pub program_name: String,
    pub max_line_bytes: usize,
    /// Timestamp prefix mode for captured child log lines.
    pub timestamp: LogTimestamp,
    /// Supervisor-style custom log path; when set, overrides `{log_dir}/{uuid}.{out|err}`.
    pub custom_path: Option<PathBuf>,
}

/// Resolve on-disk log path for a program (custom path or default UUID file).
/// Custom paths must canonicalize under `log_dir`.
pub fn resolve_log_file_path(
    log_dir: &Path,
    id: Uuid,
    source: LogSource,
    stdout_logfile: Option<&str>,
    stderr_logfile: Option<&str>,
) -> anyhow::Result<PathBuf> {
    let custom = match source {
        LogSource::Stdout => stdout_logfile,
        LogSource::Stderr => stderr_logfile,
    };
    match custom.filter(|s| !s.trim().is_empty()) {
        None => Ok(log_dir.join(format!("{}.{}", id, source.extension()))),
        Some(path) => common::resolve_confined_log_path(log_dir, path),
    }
}

/// Resolve on-disk log path for a program (custom path or default UUID file).
pub fn log_file_path(
    log_dir: &Path,
    id: Uuid,
    source: LogSource,
    stdout_logfile: Option<&str>,
    stderr_logfile: Option<&str>,
) -> PathBuf {
    resolve_log_file_path(log_dir, id, source, stdout_logfile, stderr_logfile)
        .unwrap_or_else(|_| log_dir.join(format!("{}.{}", id, source.extension())))
}

/// Read pipe stream, write to file, and broadcast over WebSocket.
#[allow(unused_assignments)]
pub fn spawn_log_consumer(
    id: Uuid,
    source: LogSource,
    mut stream: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    config: LogConfig,
    tx: broadcast::Sender<WsMessage>,
) {
    tokio::spawn(async move {
        // Fixed-size chunk reads; avoid unbounded BufReader::lines()
        let mut chunk = vec![0u8; 8192];
        let mut line_buf = Vec::new();

        // Storage setup
        let mut file_opt = None;
        #[allow(unused_assignments)]
        let mut current_size = 0;

        let file_path = config.custom_path.clone().unwrap_or_else(|| {
            config
                .log_dir
                .join(format!("{}.{}", id, source.extension()))
        });

        let rotate_dir = file_path.parent().unwrap_or(&config.log_dir).to_path_buf();
        let rotate_name = file_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("{}.{}", id, source.extension()));

        if config.driver == LogDriver::File {
            if let Some(parent) = file_path.parent() {
                if let Err(e) = fs::create_dir_all(parent).await {
                    tracing::error!("Failed to create log dir {:?}: {}", parent, e);
                    return;
                }
            } else if let Err(e) = fs::create_dir_all(&config.log_dir).await {
                tracing::error!("Failed to create log dir {:?}: {}", config.log_dir, e);
                return;
            }
            match OpenOptions::new()
                .create(true)
                .append(true)
                .open(&file_path)
                .await
            {
                Ok(f) => {
                    current_size = fs::metadata(&file_path).await.map(|m| m.len()).unwrap_or(0);
                    file_opt = Some(f);
                }
                Err(e) => {
                    tracing::error!("Failed to open log file {:?}: {}", file_path, e);
                    return;
                }
            }
        }

        let mut stdout_handle = tokio::io::stdout();
        let prefix = format!("[{}:{}] ", config.program_name, source.as_str());

        // Internal macro: unified line output (shared logic)
        macro_rules! emit_line {
            ($line_str:expr) => {
                // Timestamp prefix, captured at consume time. `None` keeps raw lines.
                let ts = match config.timestamp {
                    LogTimestamp::Local => {
                        Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string())
                    }
                    LogTimestamp::Utc => Some(Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()),
                    LogTimestamp::None => None,
                };
                // A. Stdout driver
                if config.driver == LogDriver::Stdout {
                    let output = match &ts {
                        Some(ts) => format!("[{ts}] {prefix}{line}\n", line = $line_str),
                        None => format!("{prefix}{line}\n", line = $line_str),
                    };
                    let _ = stdout_handle.write_all(output.as_bytes()).await;
                }
                // B. File driver (with rotation)
                else if let Some(file) = &mut file_opt {
                    let line_with_newline = match &ts {
                        Some(ts) => format!("[{ts}] {line}\n", line = $line_str),
                        None => format!("{line}\n", line = $line_str),
                    };
                    let bytes_len = line_with_newline.len() as u64;

                    if current_size + bytes_len > config.max_size {
                        let _ = file.flush().await;
                        if let Err(e) = rotate_logs(&rotate_dir, &rotate_name, config.backups).await
                        {
                            tracing::error!("Log rotation failed for {}: {}", rotate_name, e);
                        } else {
                            current_size = 0;
                        }

                        match OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&file_path)
                            .await
                        {
                            Ok(f) => {
                                *file = f;
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Failed to reopen log file {:?} after rotation: {}",
                                    file_path,
                                    e
                                );
                            }
                        }
                    }

                    if let Err(e) = file.write_all(line_with_newline.as_bytes()).await {
                        tracing::error!("Failed to write log for {}: {}", id, e);
                    } else {
                        current_size += bytes_len;
                    }
                }

                // C. Lazy WebSocket broadcast
                if tx.receiver_count() > 0 {
                    let msg = WsMessage::Log {
                        id,
                        source: source.as_str().to_string(),
                        line: $line_str,
                    };
                    let _ = tx.send(msg);
                }
            };
        }

        // Core bounded read loop
        loop {
            // Fixed-length reads; never unbounded memory growth
            let n = match stream.read(&mut chunk).await {
                Ok(n) if n > 0 => n,
                _ => {
                    // EOF or disconnect: flush remaining partial line
                    if !line_buf.is_empty() {
                        let final_line = String::from_utf8_lossy(&line_buf).into_owned();
                        emit_line!(final_line);
                    }
                    break;
                }
            };

            let mut start = 0;
            for i in 0..n {
                if chunk[i] == b'\n' {
                    line_buf.extend_from_slice(&chunk[start..i]);
                    // Strip trailing \r (CRLF compatibility)
                    if line_buf.last() == Some(&b'\r') {
                        line_buf.pop();
                    }

                    let final_line = String::from_utf8_lossy(&line_buf).into_owned();
                    emit_line!(final_line);

                    line_buf.clear();
                    start = i + 1;
                } else if line_buf.len() + (i - start) >= config.max_line_bytes {
                    // OOM guard: at max length, flush without waiting for newline
                    line_buf.extend_from_slice(&chunk[start..=i]);
                    let mut final_line = String::from_utf8_lossy(&line_buf).into_owned();
                    final_line.push_str("...[TRUNCATED]");

                    emit_line!(final_line);

                    line_buf.clear();
                    start = i + 1;
                }
            }
            // Buffer incomplete line tail for next iteration
            if start < n {
                line_buf.extend_from_slice(&chunk[start..n]);
            }
        }

        // Final flush
        if let Some(mut f) = file_opt {
            let _ = f.flush().await;
        }
    });
}

/// Rotate log files.
/// file.out -> file.out.1
/// file.out.1 -> file.out.2
async fn rotate_logs(dir: &Path, filename: &str, backups: u32) -> std::io::Result<()> {
    // Shift from last backup: .N -> .(N+1)
    // e.g. backups = 3: delete .3, .2 -> .3, .1 -> .2, .0 -> .1 (active -> .1)

    for i in (0..backups).rev() {
        let src_ext = if i == 0 {
            "".to_string()
        } else {
            format!(".{}", i)
        };
        let dst_ext = format!(".{}", i + 1);

        let src = dir.join(format!("{}{}", filename, src_ext));
        let dst = dir.join(format!("{}{}", filename, dst_ext));

        if src.exists() {
            // Remove destination if it exists (e.g. .3)
            if dst.exists() {
                let _ = fs::remove_file(&dst).await;
            }
            // Rename into place
            fs::rename(&src, &dst).await?;
        }
    }
    Ok(())
}

pub fn capture_stdout(
    id: Uuid,
    stream: ChildStdout,
    config: LogConfig,
    tx: broadcast::Sender<WsMessage>,
) {
    spawn_log_consumer(id, LogSource::Stdout, stream, config, tx);
}

pub fn capture_stderr(
    id: Uuid,
    stream: ChildStderr,
    config: LogConfig,
    tx: broadcast::Sender<WsMessage>,
) {
    spawn_log_consumer(id, LogSource::Stderr, stream, config, tx);
}

/// Append a superd-attributed diagnostic line to the program stderr log and broadcast over WS.
pub async fn emit_superd_line(
    id: Uuid,
    line: &str,
    log_dir: &Path,
    stdout_logfile: Option<&str>,
    stderr_logfile: Option<&str>,
    timestamp: LogTimestamp,
    tx: &broadcast::Sender<WsMessage>,
) {
    let ts = match timestamp {
        LogTimestamp::Local => Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string()),
        LogTimestamp::Utc => Some(Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()),
        LogTimestamp::None => None,
    };
    let prefixed = match ts {
        Some(ts) => format!("[{ts}] [superd] {line}"),
        None => format!("[superd] {line}"),
    };
    let path = match resolve_log_file_path(
        log_dir,
        id,
        LogSource::Stderr,
        stdout_logfile,
        stderr_logfile,
    ) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("Refusing to write stderr log for {}: {}", id, e);
            return;
        }
    };

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent).await;
    }
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await
    {
        let _ = file.write_all(format!("{prefixed}\n").as_bytes()).await;
        let _ = file.flush().await;
    }

    let _ = tx.send(WsMessage::Log {
        id,
        source: "superd".to_string(),
        line: prefixed,
    });
}

/// Read last N lines from a log file (historical log API).
pub async fn read_log_lines(
    log_dir: &Path,
    id: Uuid,
    source: LogSource,
    max_lines: u32,
    stdout_logfile: Option<&str>,
    stderr_logfile: Option<&str>,
) -> Option<String> {
    let path = resolve_log_file_path(log_dir, id, source, stdout_logfile, stderr_logfile).ok()?;
    if !path.exists() {
        return None;
    }

    let content = tokio::fs::read_to_string(&path).await.ok()?;
    if content.is_empty() {
        return None;
    }

    let lines: Vec<&str> = content.lines().collect();
    let n = max_lines as usize;
    let start = lines.len().saturating_sub(n);
    Some(lines[start..].join("\n"))
}

/// Read last N bytes of log file (alert snapshot; File driver only).
pub async fn read_log_tail(
    log_dir: &Path,
    id: Uuid,
    source: LogSource,
    max_bytes: u64,
    stdout_logfile: Option<&str>,
    stderr_logfile: Option<&str>,
) -> Option<String> {
    let path = resolve_log_file_path(log_dir, id, source, stdout_logfile, stderr_logfile).ok()?;

    if !path.exists() {
        return None;
    }

    // Sync IO for a small tail read is acceptable here; keeps the runtime simple.
    match tokio::fs::File::open(&path).await {
        Ok(mut file) => {
            let len = file.metadata().await.ok()?.len();
            if len == 0 {
                return None;
            }

            // let start = if len > max_bytes { len - max_bytes } else { 0 };
            let start = len.saturating_sub(max_bytes);

            if file.seek(SeekFrom::Start(start)).await.is_err() {
                return None;
            }

            let mut buffer = vec![0; max_bytes as usize];
            let n = file.read(&mut buffer).await.ok()?;

            // Decode UTF-8, lossy on invalid bytes
            let s = String::from_utf8_lossy(&buffer[..n]).to_string();
            Some(s)
        }
        Err(_) => None,
    }
}
