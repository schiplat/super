-- CLI device-code sign-in. The CLI never sees the browser session: it holds
-- an opaque device code (hash stored here) and displays a short user code the
-- browser confirms. Approval copies the browser session's identity claims, and
-- the poll creates a fresh session for the CLI (the token is generated at poll
-- time and never stored here).

CREATE TABLE cli_pending (
    device_sha256  BLOB PRIMARY KEY,
    user_code      TEXT NOT NULL UNIQUE,
    created_at     INTEGER NOT NULL,
    expires_at     INTEGER NOT NULL,
    approved_at    INTEGER,
    sub            TEXT,
    email          TEXT,
    name           TEXT,
    groups_json    TEXT,
    email_verified INTEGER
);
CREATE INDEX cli_pending_expires ON cli_pending(expires_at);
