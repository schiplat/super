use anyhow::Context;
use common::{EventStats, ProgramEventRecord};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::path::Path;
use std::str::FromStr;
use uuid::Uuid;

/// Event history persistence backed by SQLite.
///
/// `events.db` is the **source of truth** for lifecycle/run history. All events
/// are recorded (not just anomalies); retention is unlimited by default, with an
/// optional `events_keep_days` window for time-based pruning. Queries (filter,
/// stats, aggregation) run as SQL so large histories stay efficient.
#[derive(Clone)]
pub struct EventDb {
    pool: SqlitePool,
}

/// Filters for `EventDb::query`.
#[derive(Debug, Clone, Default)]
pub struct EventQuery {
    pub program_id: Option<Uuid>,
    /// Inclusive lower bound on `ts` (Unix seconds).
    pub from: Option<u64>,
    /// Inclusive upper bound on `ts` (Unix seconds).
    pub to: Option<u64>,
    pub event_type: Option<String>,
    pub exit_code: Option<i32>,
    /// Free-text match on `msg`.
    pub q: Option<String>,
    /// Row limit (oldest-first ordering after filters).
    pub limit: Option<u32>,
    /// Offset for pagination.
    pub offset: Option<u32>,
}

impl EventDb {
    /// Open (or create) the SQLite store at `path`, applying migrations.
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("create data dir for events db: {}", parent.display()))?;
        }
        let conn = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.to_string_lossy()))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .busy_timeout(std::time::Duration::from_secs(5))
            .foreign_keys(true)
            // High-throughput tuning for the batch writer + query workload.
            .pragma("cache_size", "-65536") // 64 MiB shared page cache
            .pragma("journal_size_limit", "67108864") // cap WAL growth at 64 MiB
            .pragma("temp_store", "2") // sorts live in memory, not temp files
            .pragma("mmap_size", "268435456") // mmap reads for long-running queries
            .pragma("wal_autocheckpoint", "4000") // batch checkpoints instead of per-commit
            .statement_cache_capacity(256); // reuse prepared INSERT/SELECT plans

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(conn)
            .await
            .context("open events database")?;

        // Apply embedded migrations (PRAGMA user_version tracked).
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .context("run events database migrations")?;

        Ok(Self { pool })
    }

    /// Persist a single event.
    pub async fn insert(&self, e: &ProgramEventRecord) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO events
               (program_id, program_name, ts, ts_ms, event, exit_code, signal, retry_count, duration_secs, msg)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"#,
        )
        .bind(e.program_id.map(|id| id.to_string()))
        .bind(e.program_name.as_deref())
        .bind(e.ts as i64)
        .bind(e.ts_ms as i64)
        .bind(&e.event)
        .bind(e.exit_code)
        .bind(e.signal)
        .bind(e.retry_count.map(|v| v as i64))
        .bind(e.duration_secs.map(|v| v as i64))
        .bind(&e.msg)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Insert many events in a single transaction (batch flush).
    pub async fn insert_batch(&self, events: &[ProgramEventRecord]) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        for e in events {
            sqlx::query(
                r#"INSERT INTO events
                   (program_id, program_name, ts, ts_ms, event, exit_code, signal, retry_count, duration_secs, msg)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"#,
            )
            .bind(e.program_id.map(|id| id.to_string()))
            .bind(e.program_name.as_deref())
            .bind(e.ts as i64)
            .bind(e.ts_ms as i64)
            .bind(&e.event)
            .bind(e.exit_code)
            .bind(e.signal)
            .bind(e.retry_count.map(|v| v as i64))
            .bind(e.duration_secs.map(|v| v as i64))
            .bind(&e.msg)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Query events with filters, newest first.
    pub async fn query(&self, q: &EventQuery) -> anyhow::Result<Vec<ProgramEventRecord>> {
        let mut sql = String::from(
            r#"SELECT program_id, program_name, ts, ts_ms, event, exit_code, signal,
                       retry_count, duration_secs, msg
                 FROM events
                 WHERE 1=1"#,
        );
        if q.program_id.is_some() {
            sql.push_str(" AND program_id = ?");
        }
        if q.from.is_some() {
            sql.push_str(" AND ts >= ?");
        }
        if q.to.is_some() {
            sql.push_str(" AND ts <= ?");
        }
        if q.event_type.is_some() {
            sql.push_str(" AND event = ?");
        }
        if q.exit_code.is_some() {
            sql.push_str(" AND exit_code = ?");
        }
        if q.q.is_some() {
            sql.push_str(" AND msg LIKE ?");
        }
        sql.push_str(" ORDER BY ts_ms ASC, id ASC");
        if q.limit.is_some() {
            sql.push_str(" LIMIT ?");
        }
        if q.offset.is_some() {
            sql.push_str(" OFFSET ?");
        }

        let mut qb = sqlx::query(&sql);
        if let Some(pid) = q.program_id {
            qb = qb.bind(pid.to_string());
        }
        if let Some(from) = q.from {
            qb = qb.bind(from as i64);
        }
        if let Some(to) = q.to {
            qb = qb.bind(to as i64);
        }
        if let Some(et) = &q.event_type {
            qb = qb.bind(et.clone());
        }
        if let Some(code) = q.exit_code {
            qb = qb.bind(code);
        }
        if let Some(text) = &q.q {
            qb = qb.bind(format!("%{}%", text));
        }
        if let Some(lim) = q.limit {
            qb = qb.bind(lim as i64);
        }
        if let Some(off) = q.offset {
            qb = qb.bind(off as i64);
        }

        let rows = qb.fetch_all(&self.pool).await?;
        Ok(rows.iter().map(row_to_event).collect())
    }

    /// Retention statistics, optionally scoped to one program.
    pub async fn stats(&self, program_id: Option<Uuid>) -> anyhow::Result<EventStats> {
        let mut stats = EventStats::default();

        let (total_sql, first_sql, last_sql) = if program_id.is_some() {
            (
                "SELECT COUNT(*) FROM events WHERE program_id = ?",
                "SELECT MIN(ts) FROM events WHERE program_id = ?",
                "SELECT MAX(ts) FROM events WHERE program_id = ?",
            )
        } else {
            (
                "SELECT COUNT(*) FROM events",
                "SELECT MIN(ts) FROM events",
                "SELECT MAX(ts) FROM events",
            )
        };

        let mut cnt = sqlx::query(total_sql);
        let mut fst = sqlx::query(first_sql);
        let mut lst = sqlx::query(last_sql);
        if let Some(pid) = program_id {
            let s = pid.to_string();
            cnt = cnt.bind(s.clone());
            fst = fst.bind(s.clone());
            lst = lst.bind(s);
        }
        stats.total = cnt.fetch_one(&self.pool).await?.try_get::<i64, _>(0)? as u64;
        stats.first_ts = fst
            .fetch_one(&self.pool)
            .await?
            .try_get::<Option<i64>, _>(0)?
            .map(|v| v as u64);
        stats.last_ts = lst
            .fetch_one(&self.pool)
            .await?
            .try_get::<Option<i64>, _>(0)?
            .map(|v| v as u64);

        let by_type_sql = if program_id.is_some() {
            "SELECT event, COUNT(*) AS c FROM events WHERE program_id = ? GROUP BY event ORDER BY c DESC"
        } else {
            "SELECT event, COUNT(*) AS c FROM events GROUP BY event ORDER BY c DESC"
        };
        let mut bt = sqlx::query(by_type_sql);
        if let Some(pid) = program_id {
            bt = bt.bind(pid.to_string());
        }
        for row in bt.fetch_all(&self.pool).await? {
            stats.by_type.push(common::EventTypeCount {
                event: row.try_get("event")?,
                count: row.try_get::<i64, _>("c")? as u64,
            });
        }
        Ok(stats)
    }

    /// Prune events older than `keep_days` (0 = keep everything). Returns the
    /// number of deleted rows.
    pub async fn prune_older_than(&self, keep_days: u64) -> anyhow::Result<u64> {
        if keep_days == 0 {
            return Ok(0);
        }
        let cutoff = chrono::Utc::now().timestamp() - (keep_days as i64) * 86_400;
        let res = sqlx::query("DELETE FROM events WHERE ts < ?")
            .bind(cutoff)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }

    /// Remove every event for a program (used on program removal).
    pub async fn delete_program(&self, program_id: Uuid) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM events WHERE program_id = ?")
            .bind(program_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn row_to_event(row: &sqlx::sqlite::SqliteRow) -> ProgramEventRecord {
    ProgramEventRecord {
        ts: row.get::<i64, _>("ts") as u64,
        ts_ms: row.get::<i64, _>("ts_ms") as u64,
        program_id: row
            .get::<Option<String>, _>("program_id")
            .and_then(|s| Uuid::parse_str(&s).ok()),
        program_name: row.get::<Option<String>, _>("program_name"),
        event: row.get::<String, _>("event"),
        exit_code: row.get::<Option<i64>, _>("exit_code").map(|v| v as i32),
        signal: row.get::<Option<i64>, _>("signal").map(|v| v as i32),
        retry_count: row.get::<Option<i64>, _>("retry_count").map(|v| v as u32),
        duration_secs: row.get::<Option<i64>, _>("duration_secs").map(|v| v as u64),
        msg: row.get::<String, _>("msg"),
    }
}
