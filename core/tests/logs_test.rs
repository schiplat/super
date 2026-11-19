use super_core::logger::{self, LogSource};
use uuid::Uuid;

#[tokio::test]
async fn test_read_log_lines_tail() {
    let dir = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let path = dir.path().join(format!("{}.out", id));
    let content = (1..=20)
        .map(|i| format!("line-{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    tokio::fs::write(&path, format!("{content}\n"))
        .await
        .unwrap();

    let tail = logger::read_log_lines(dir.path(), id, LogSource::Stdout, 5, None, None)
        .await
        .unwrap();
    let lines: Vec<&str> = tail.lines().collect();
    assert_eq!(lines.len(), 5);
    assert_eq!(lines[0], "line-16");
    assert_eq!(lines[4], "line-20");
}

#[tokio::test]
async fn test_read_log_lines_custom_path() {
    let dir = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let custom = dir.path().join("myapp.stdout.log");
    tokio::fs::write(&custom, "custom-log-line\n")
        .await
        .unwrap();

    let tail = logger::read_log_lines(
        dir.path(),
        id,
        LogSource::Stdout,
        10,
        Some(custom.to_str().unwrap()),
        None,
    )
    .await
    .unwrap();
    assert_eq!(tail.trim(), "custom-log-line");
}

#[tokio::test]
async fn test_read_log_lines_rejects_path_outside_log_dir() {
    let dir = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let outside = std::env::temp_dir().join(format!("super-outside-{}.log", id));
    tokio::fs::write(&outside, "secret\n").await.unwrap();

    let tail = logger::read_log_lines(
        dir.path(),
        id,
        LogSource::Stdout,
        10,
        Some(outside.to_str().unwrap()),
        None,
    )
    .await;
    assert!(tail.is_none());
    let _ = tokio::fs::remove_file(outside).await;
}

/// The backwards tail scan must match a full-file read even when the file
/// spans multiple 64 KiB chunks, and must not touch lines it does not need.
/// (The old implementation slurped the whole file into memory per request.)
#[tokio::test]
async fn test_read_log_lines_multi_chunk_file() {
    let dir = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let path = dir.path().join(format!("{}.out", id));

    // 30000 short lines (~200 KB) spans several 64 KiB chunks.
    let total = 30000usize;
    let mut body = String::with_capacity(total * 8);
    for i in 0..total {
        body.push_str(&format!("line-{i}\n"));
    }
    tokio::fs::write(&path, &body).await.unwrap();

    let tail = logger::read_log_lines(dir.path(), id, LogSource::Stdout, 10, None, None)
        .await
        .unwrap();
    let expected: Vec<String> = ((total - 10)..total).map(|i| format!("line-{i}")).collect();
    assert_eq!(tail, expected.join("\n"));

    // Whole-file request still returns everything.
    let all = logger::read_log_lines(dir.path(), id, LogSource::Stdout, 50000, None, None)
        .await
        .unwrap();
    assert_eq!(all.lines().count(), total);
    assert!(all.starts_with("line-0"));
}
