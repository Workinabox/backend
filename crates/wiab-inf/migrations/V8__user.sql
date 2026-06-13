-- User aggregate root. `version` is the optimistic-concurrency token
-- (incremented on every save; guarded with `WHERE version = $expected`).
-- Table is `app_user` because `user` is reserved in Postgres. Ids are the
-- canonical string form (e.g. "U-1"). No FK constraints: parent existence is
-- enforced in the application layer.
CREATE TABLE app_user (
    id       TEXT PRIMARY KEY,
    version  BIGINT NOT NULL,
    kind     TEXT NOT NULL,
    name     TEXT NOT NULL,
    email    TEXT,
    agent_id TEXT
);

-- Owned SSH keys, ordered by `position` within the owning user.
CREATE TABLE user_ssh_key (
    user_id            TEXT NOT NULL,
    position           INTEGER NOT NULL,
    id                 TEXT NOT NULL,
    label              TEXT NOT NULL,
    openssh_public_key TEXT NOT NULL,
    fingerprint        TEXT NOT NULL,
    PRIMARY KEY (user_id, position)
);

-- Owned access tokens, ordered by `position` within the owning user.
-- `scope` holds the JSON-encoded TokenScope.
CREATE TABLE user_access_token (
    user_id      TEXT NOT NULL,
    position     INTEGER NOT NULL,
    id           TEXT NOT NULL,
    label        TEXT NOT NULL,
    hash         TEXT NOT NULL,
    display      TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    expires_at   TEXT,
    last_used_at TEXT,
    scope        TEXT NOT NULL,
    PRIMARY KEY (user_id, position)
);
