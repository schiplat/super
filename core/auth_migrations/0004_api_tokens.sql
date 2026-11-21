-- Named API tokens for OSS-mode automation (the `super token` CLI commands
-- target `/api/v1/auth/tokens`, which the security plugin serves in licensed
-- mode and core serves in OSS mode). Only the SHA-256 hash is stored; the
-- plaintext is shown exactly once at creation. A token authenticates with its
-- own role under the static-secret gate and is dormant once SSO owns the gate
-- (exactly like the break-glass secret itself).

CREATE TABLE api_tokens (
    id            TEXT PRIMARY KEY,
    token_sha256  BLOB NOT NULL UNIQUE,
    -- Display prefix (first chars of the plaintext + ellipsis): the plaintext
    -- itself is never stored, but users need to tell tokens apart.
    token_prefix  TEXT NOT NULL DEFAULT '',
    name          TEXT NOT NULL,
    role          TEXT NOT NULL,
    created_by    TEXT NOT NULL DEFAULT '',
    created_at    INTEGER NOT NULL,
    last_used_at  INTEGER,
    revoked_at    INTEGER
);

CREATE INDEX api_tokens_revoked ON api_tokens(revoked_at);
