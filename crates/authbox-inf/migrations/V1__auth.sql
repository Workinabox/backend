-- authbox auth schema. Tracked in its own refinery history table (`authbox_migrations`)
-- so this series is independent of the host's migrations and extracts cleanly later.
-- Ids/timestamps are canonical strings (RFC3339 for timestamps, compared lexically).

-- One password credential per principal (the host's user id, e.g. "U-1").
CREATE TABLE user_password (
    user_id    TEXT PRIMARY KEY,
    phc_hash   TEXT NOT NULL,
    state      TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Server-side browser sessions. Resolved by `token_hash` (the hash of the cookie secret;
-- the secret itself is never stored). Not versioned — last-write-wins.
CREATE TABLE auth_session (
    id                  TEXT PRIMARY KEY,
    token_hash          TEXT NOT NULL,
    csrf_hash           TEXT NOT NULL,
    principal_id        TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    last_seen_at        TEXT NOT NULL,
    idle_expires_at     TEXT NOT NULL,
    absolute_expires_at TEXT NOT NULL,
    revoked             BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE UNIQUE INDEX idx_auth_session_token_hash ON auth_session (token_hash);
CREATE INDEX idx_auth_session_principal ON auth_session (principal_id);
