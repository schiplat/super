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
    let stats = db.stats(None).await.unwrap();
    assert_eq!(stats.total, total);
}
