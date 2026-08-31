use common::SystemEvent;
use common::config::EventHookConfig;
use std::sync::{Arc, Mutex};
use super_core::event_hooks;
use uuid::Uuid;

#[tokio::test]
async fn test_event_hook_receives_json_on_stdin() {
    let dir = tempfile::tempdir().unwrap();
    let out_file = dir.path().join("hook.out");
    let out = out_file.display().to_string();

    let script = format!(r#"read line; printf '%s' "$line" > "{out}""#, out = out);

    let hook = EventHookConfig {
        command: script,
        url: None,
        headers: None,
        events: vec!["*".to_string()],
        programs: vec!["*".to_string()],
        r#async: false,
        timeout_secs: 5,
        id: Some("test-hook".to_string()),
    };

    let event = SystemEvent::ProcessStarted {
        program_id: Uuid::new_v4(),
        program_name: "demo".to_string(),
        pid: 4242,
    };

    event_hooks::dispatch(&[hook], &event);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let body = std::fs::read_to_string(out_file).unwrap();
    assert!(
        body.contains("\"event\":\"process_started\""),
        "body={body}"
    );
    assert!(body.contains("\"name\":\"demo\""), "body={body}");
    assert!(body.contains("4242"), "body={body}");
}

#[derive(Clone)]
struct RecordingExtension {
    events: Arc<Mutex<Vec<SystemEvent>>>,
}

impl super_core::extension::Extension for RecordingExtension {
    fn on_event(&self, event: SystemEvent) {
        self.events.lock().unwrap().push(event);
    }
}

#[tokio::test]
async fn test_emit_notifies_extension_and_runs_hook() {
    let ext = RecordingExtension {
        events: Arc::new(Mutex::new(Vec::new())),
    };
    let extension: Arc<dyn super_core::extension::Extension> = Arc::new(ext.clone());

    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("ran");
    let marker_str = marker.display().to_string();

    let hooks = vec![EventHookConfig {
        command: format!(r#"touch "{}""#, marker_str),
        url: None,
        headers: None,
        events: vec!["system_shutdown".to_string()],
        programs: vec!["*".to_string()],
        r#async: false,
        timeout_secs: 5,
        id: None,
    }];

    event_hooks::emit(&extension, &hooks, SystemEvent::SystemShutdown);

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    assert!(marker.exists());
    assert_eq!(ext.events.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn test_webhook_hook_posts_json() {
    // Minimal HTTP server capturing one POST request
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        use std::io::{Read, Write};
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        let header_end;
        loop {
            let n = stream.read(&mut chunk).unwrap();
            if n == 0 {
                return String::from_utf8_lossy(&buf).to_string();
            }
            buf.extend_from_slice(&chunk[..n]);
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                header_end = pos + 4;
                break;
            }
        }
        let head = String::from_utf8_lossy(&buf[..header_end]).to_lowercase();
        let content_len: usize = head
            .lines()
            .find_map(|l| l.strip_prefix("content-length:"))
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0);
        while buf.len() < header_end + content_len {
            let n = stream.read(&mut chunk).unwrap();
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
        String::from_utf8_lossy(&buf).to_string()
    });

    let url = format!("http://{addr}/hook");
    let hook = EventHookConfig {
        command: "".to_string(),
        url: Some(url),
        headers: None,
        events: vec!["*".to_string()],
        programs: vec!["*".to_string()],
        r#async: false,
        timeout_secs: 5,
        id: Some("test-webhook".to_string()),
    };

    let event = SystemEvent::ProcessStarted {
        program_id: Uuid::new_v4(),
        program_name: "demo".to_string(),
        pid: 4242,
    };

    event_hooks::dispatch(&[hook], &event);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let request = handle.join().unwrap();
    assert!(request.contains("POST /hook"), "request={request}");
    assert!(
        request.contains("\"event\":\"process_started\""),
        "request={request}"
    );
    assert!(request.contains("\"name\":\"demo\""), "request={request}");
}

#[test]
fn test_memory_events_json_roundtrip_and_payload() {
    // Plugin→host channel serializes SystemEvent to JSON; the host must
    // deserialize it back exactly. Verify both memory event variants.
    let id = Uuid::new_v4();
    let pressure = SystemEvent::MemoryPressure {
        program_id: id,
        program_name: "api".to_string(),
        pid: Some(77),
        usage_bytes: 600 * 1024 * 1024,
        limit_bytes: 512 * 1024 * 1024,
        warn_bytes: 400 * 1024 * 1024,
    };
    let json = serde_json::to_string(&pressure).unwrap();
    let back: SystemEvent = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, SystemEvent::MemoryPressure { .. }));
    assert_eq!(back.event_type(), "memory_pressure");
    assert_eq!(back.program_name(), Some("api"));

    let oom = SystemEvent::MemoryOomKill {
        program_id: id,
        program_name: "api".to_string(),
        pid: None,
        anon_bytes: 700 * 1024 * 1024,
        limit_bytes: 512 * 1024 * 1024,
        usage_bytes: 900 * 1024 * 1024,
    };
    let json = serde_json::to_string(&oom).unwrap();
    let back: SystemEvent = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, SystemEvent::MemoryOomKill { .. }));
    assert_eq!(back.event_type(), "memory_oom_kill");
    assert_eq!(back.program_name(), Some("api"));
}

#[test]
fn test_memory_event_hook_payload_uses_bytes() {
    // Webhook payload keeps kernel-native byte units even though config is MB.
    let payload = super_core::event_hooks::build_payload(&SystemEvent::MemoryPressure {
        program_id: Uuid::new_v4(),
        program_name: "db".to_string(),
        pid: Some(3),
        usage_bytes: 100 * 1024 * 1024,
        limit_bytes: 128 * 1024 * 1024,
        warn_bytes: 102 * 1024 * 1024,
    })
    .unwrap();
    let body: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(body["event"], "memory_pressure");
    assert_eq!(body["program"]["name"], "db");
    assert_eq!(body["payload"]["usage_bytes"], 100 * 1024 * 1024);
    assert_eq!(body["payload"]["limit_bytes"], 128 * 1024 * 1024);
    assert_eq!(body["payload"]["warn_bytes"], 102 * 1024 * 1024);
}
