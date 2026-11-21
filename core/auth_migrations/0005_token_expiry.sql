-- Optional expiry for API tokens. NULL = no expiry (the default). Expired
-- tokens stay visible in the management list (so an operator can see why
-- authentication failed) but never authenticate again.

ALTER TABLE api_tokens ADD COLUMN expires_at INTEGER;
