CREATE TABLE IF NOT EXISTS events (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    program_id    TEXT,              -- NULL for system-wide events
    program_name  TEXT,
    ts            INTEGER NOT NULL,  -- Unix seconds (compat)
    ts_ms         INTEGER NOT NULL,  -- Unix milliseconds (precise, sort key)
    event         TEXT    NOT NULL,  -- process_fatal / cron_started / ... (SystemEvent.event_type aligned)
    exit_code     INTEGER,
    signal        INTEGER,
    retry_count   INTEGER,
    duration_secs INTEGER,           -- cron_exit run duration
    msg           TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_events_program_ts ON events(program_id, ts DESC);
CREATE INDEX IF NOT EXISTS idx_events_ts         ON events(ts DESC);
CREATE INDEX IF NOT EXISTS idx_events_event_ts   ON events(event, ts DESC);
