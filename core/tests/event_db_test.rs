//! EventDb query bounds (kept in a dedicated file per the manager-tests rule).

use common::ProgramEventRecord;
use tempfile::TempDir;

async fn open_db(dir: &TempDir) -> super_core::event_db::EventDb {
    super_core::event_db::EventDb::open(&dir.path().join("events.db"))
        .await
        .unwrap()
}

fn record(i: u64) -> ProgramEventRecord {
    ProgramEventRecord {
        ts: 1000 + i,
        ts_ms: (1000 + i) * 1000,
        program_id: None,
        program_name: Some(format!("app-{}", i % 3)),
        event: "process_exit".to_string(),
        exit_code: Some(0),
        signal: None,
        retry_count: None,
        duration_secs: None,
        msg: format!("event {i}"),
    }
}

/// `limit=u32::MAX` must not select the whole table: the query layer clamps
/// the LIMIT to a sane page size. Rows survive in the DB for later pages.
#[tokio::test]
async fn query_clamps_huge_limit() {
    let dir = TempDir::new().unwrap();
    let db = open_db(&dir).await;

    let total = 5010u64;
    let batch: Vec<ProgramEventRecord> = (0..total).map(record).collect();
    db.insert_batch(&batch).await.unwrap();

    let q = super_core::event_db::EventQuery {
        limit: Some(u32::MAX),
        ..Default::default()
    };
    let rows = db.query(&q).await.unwrap();
    assert_eq!(
        rows.len(),
        5000,
        "huge limit must be clamped to the query cap"
    );

    // Pagination past the clamp still reaches the remaining rows.
    let rest = db
        .query(&super_core::event_db::EventQuery {
            limit: Some(u32::MAX),
            offset: Some(5000),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(rest.len(), 10);

    // Stats still see everything (clamping affects LIMIT only).
    let stats = db.stats(None, None, None).await.unwrap();
    assert_eq!(stats.total, total);
}

/// Stats honour the time window (`from`/`to`), including `by_type`, so the
/// events page summary matches the list it is glued to.
#[tokio::test]
async fn stats_respect_the_time_window() {
    let dir = tempfile::tempdir().unwrap();
    let db = super_core::event_db::EventDb::open(&dir.path().join("events.db"))
        .await
        .unwrap();
    let base = 1_700_000_000u64;
    for (index, ts) in [base, base + 60, base + 120].into_iter().enumerate() {
        db.insert(&ProgramEventRecord {
            program_id: None,
            program_name: Some("demo".into()),
            ts,
            ts_ms: ts * 1000,
            event: if index == 0 { "start" } else { "exit" }.into(),
            exit_code: None,
            signal: None,
            retry_count: None,
            duration_secs: None,
            msg: "test".into(),
        })
        .await
        .unwrap();
    }

    let all = db.stats(None, None, None).await.unwrap();
    assert_eq!(all.total, 3);
    assert_eq!(all.first_ts, Some(base));
    assert_eq!(all.last_ts, Some(base + 120));

    let window = db
        .stats(None, Some(base + 60), Some(base + 120))
        .await
        .unwrap();
    assert_eq!(window.total, 2);
    assert_eq!(window.first_ts, Some(base + 60));
    assert_eq!(window.last_ts, Some(base + 120));
    assert_eq!(window.by_type.len(), 1, "{:?}", window.by_type);
    assert_eq!(window.by_type[0].event, "exit");
    assert_eq!(window.by_type[0].count, 2);

    let empty = db.stats(None, Some(base + 500), None).await.unwrap();
    assert_eq!(empty.total, 0);
    assert!(empty.by_type.is_empty());
}
