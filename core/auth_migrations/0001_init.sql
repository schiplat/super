-- SSO configuration, OIDC client secret, local sessions, pending
-- authorization state, and login audit. One store, owner-only file: every row
-- here is credential-grade or decides who gets in.
--
-- `revision` is bumped on every config write so an out-of-process writer (the
-- CLI in --offline mode) can be noticed by polling one integer.

CREATE TABLE IF NOT EXISTS sso_config (
    id                    INTEGER PRIMARY KEY CHECK (id = 1),
    enabled               INTEGER NOT NULL DEFAULT 0,
    issuer                TEXT NOT NULL,
    client_id             TEXT NOT NULL,
    public_base_url       TEXT NOT NULL,
    scopes                TEXT NOT NULL,
    groups_claim          TEXT NOT NULL,
    token_auth_method     TEXT NOT NULL,
    role_rules            TEXT NOT NULL,
    session_ttl_secs      INTEGER NOT NULL,
    idle_timeout_secs     INTEGER NOT NULL,
    allow_insecure_issuer INTEGER NOT NULL DEFAULT 0,
    revision              INTEGER NOT NULL DEFAULT 0,
    updated_at            INTEGER NOT NULL,
    updated_by            TEXT
);

CREATE TABLE IF NOT EXISTS sso_secret (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    secret              TEXT NOT NULL,
    previous_secret     TEXT,
    previous_expires_at INTEGER,
    updated_at          INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    id               TEXT PRIMARY KEY,
    token_sha256     BLOB NOT NULL UNIQUE,
    sub              TEXT NOT NULL,
    email            TEXT,
    name             TEXT,
    groups_json      TEXT NOT NULL DEFAULT '[]',
    created_at       INTEGER NOT NULL,
    expires_at       INTEGER NOT NULL,
    last_seen_at     INTEGER NOT NULL,
    revoked_at       INTEGER,
    exchange_used_at INTEGER,
    ip               TEXT,
    user_agent       TEXT
);
CREATE INDEX IF NOT EXISTS sessions_expires ON sessions(expires_at);
CREATE INDEX IF NOT EXISTS sessions_sub ON sessions(sub);

CREATE TABLE IF NOT EXISTS pending (
    state       TEXT PRIMARY KEY,
    nonce       TEXT NOT NULL,
    verifier    TEXT NOT NULL,
    return_to   TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    expires_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS pending_expires ON pending(expires_at);

CREATE TABLE IF NOT EXISTS auth_events (
    id     INTEGER PRIMARY KEY AUTOINCREMENT,
    at     INTEGER NOT NULL,
    kind   TEXT NOT NULL,
    sub    TEXT,
    email  TEXT,
    ip     TEXT,
    ua     TEXT,
    detail TEXT
);
CREATE INDEX IF NOT EXISTS auth_events_at ON auth_events(at);
