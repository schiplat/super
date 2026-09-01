-- Rebuild event indexes to align with the (ts_ms, id) sort order used by
-- EventDb::query (and the ts_ms-based time-window filters). The previous
-- indexes were keyed on second-granularity ts, so ORDER BY ts_ms, id had to
-- sort rows after fetching.
DROP INDEX IF EXISTS idx_events_program_ts;
DROP INDEX IF EXISTS idx_events_ts;
DROP INDEX IF EXISTS idx_events_event_ts;

CREATE INDEX IF NOT EXISTS idx_events_program_ts_ms ON events(program_id, ts_ms, id);
CREATE INDEX IF NOT EXISTS idx_events_ts_ms         ON events(ts_ms, id);
CREATE INDEX IF NOT EXISTS idx_events_event_ts_ms   ON events(event, ts_ms, id);
