use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("super-doctor-test-{id}"));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct MockDaemon {
    addr: SocketAddr,
    degraded: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl MockDaemon {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let degraded = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_degraded = Arc::clone(&degraded);
        let thread_stop = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => serve(stream, &thread_degraded),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            addr,
            degraded,
            stop,
            thread: Some(thread),
        }
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for MockDaemon {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.addr);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(mut stream: TcpStream, degraded: &AtomicBool) {
    let mut request = [0_u8; 2048];
    let count = stream.read(&mut request).unwrap_or(0);
    let request = String::from_utf8_lossy(&request[..count]);
    if request.starts_with("GET /health ") {
        let is_degraded = degraded.load(Ordering::Relaxed);
        let status = if is_degraded { "degraded" } else { "healthy" };
        let code = if is_degraded {
            "503 Service Unavailable"
        } else {
            "200 OK"
        };
        let body = format!(r#"{{"status":"{status}","components":{{"web":"up"}}}}"#);
        write_response(&mut stream, code, &body);
    } else if request.starts_with("GET /api/v1/system/license ") {
        write_response(&mut stream, "404 Not Found", r#"{"message":"not found"}"#);
    } else {
        write_response(&mut stream, "404 Not Found", "{}");
    }
}

fn write_response(stream: &mut TcpStream, status: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn run_super(root: &Path, args: &[&str]) -> Output {
    run_super_from(root, root, args)
}

fn run_super_from(root: &Path, cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_super"))
        .args(args)
        .env("SUPER_ROOT", root)
        .env_remove("SUPER_TOKEN")
        .current_dir(cwd)
        .output()
        .unwrap()
}

fn write_config(root: &Path, port: u16, socket_mode: &str) {
    fs::create_dir_all(root.join("conf")).unwrap();
    fs::create_dir_all(root.join("logs")).unwrap();
    fs::create_dir_all(root.join("data")).unwrap();
    fs::write(root.join("data/snapshot.json"), "{}").unwrap();
    fs::write(root.join("data/events.db"), "").unwrap();
    fs::write(
        root.join("conf/super.toml"),
        format!(
            "[server]\nhost = \"127.0.0.1\"\nport = {port}\nsocket = \"run/superd.sock\"\nsocket_mode = \"{socket_mode}\"\n\n[storage]\nlog_dir = \"logs\"\ndata_file = \"data/snapshot.json\"\nevents_file = \"data/events.db\"\n",
        ),
    )
    .unwrap();
}

#[test]
fn doctor_skips_live_port_probe_and_reports_unhealthy_daemon() {
    static SERIAL: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
    let _serial = SERIAL
        .get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap();
    let root = TempRoot::new();
    let unrelated_cwd = TempRoot::new();
    let daemon = MockDaemon::start();
    write_config(root.path(), daemon.addr.port(), "0600");

    let doctor_ok = run_super_from(
        root.path(),
        unrelated_cwd.path(),
        &["--server", &daemon.url(), "doctor"],
    );
    assert!(
        doctor_ok.status.success(),
        "healthy OSS daemon should pass doctor despite occupying its configured port:\n{}\n{}",
        String::from_utf8_lossy(&doctor_ok.stdout),
        String::from_utf8_lossy(&doctor_ok.stderr)
    );
    assert!(String::from_utf8_lossy(&doctor_ok.stdout).contains("healthy"));
    let doctor_stdout = String::from_utf8_lossy(&doctor_ok.stdout);
    assert!(doctor_stdout.contains(&root.path().join("logs").display().to_string()));
    assert!(doctor_stdout.contains(&root.path().join("data/snapshot.json").display().to_string()));
    assert!(doctor_stdout.contains(&root.path().join("run/superd.sock").display().to_string()));
    assert!(doctor_stdout.contains("port probe skipped"));
    assert!(
        !doctor_stdout.contains(&unrelated_cwd.path().join("logs").display().to_string()),
        "doctor must not resolve instance paths under the caller's current directory"
    );

    let check_unchanged = run_super_from(root.path(), unrelated_cwd.path(), &["check"]);
    assert!(
        !check_unchanged.status.success(),
        "super check should continue flagging an occupied configured port"
    );
    assert!(
        String::from_utf8_lossy(&check_unchanged.stdout).contains("likely in use"),
        "super check should report its occupied-port diagnostic"
    );

    daemon.degraded.store(true, Ordering::Relaxed);
    let doctor_degraded = run_super(root.path(), &["--server", &daemon.url(), "doctor"]);
    assert!(
        !doctor_degraded.status.success(),
        "degraded daemon health must produce a non-zero doctor exit status"
    );
    assert!(String::from_utf8_lossy(&doctor_degraded.stdout).contains("degraded"));

    let unavailable_port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    drop(TcpListener::bind(("127.0.0.1", unavailable_port)).unwrap());
    let doctor_unreachable = run_super(
        root.path(),
        &[
            "--server",
            &format!("http://127.0.0.1:{unavailable_port}"),
            "doctor",
        ],
    );
    assert!(
        !doctor_unreachable.status.success(),
        "unreachable daemon must produce a non-zero doctor exit status"
    );

    daemon.degraded.store(false, Ordering::Relaxed);
    write_config(root.path(), daemon.addr.port(), "0777");
    let doctor_invalid_config = run_super(root.path(), &["--server", &daemon.url(), "doctor"]);
    assert!(
        !doctor_invalid_config.status.success(),
        "invalid configuration must produce a non-zero doctor exit status"
    );
}
