CREATE TABLE pull_request (
    id            TEXT PRIMARY KEY,
    version       BIGINT NOT NULL,
    repo_id       TEXT NOT NULL,
    author_id     TEXT NOT NULL,
    title         TEXT NOT NULL,
    description   TEXT NOT NULL,
    source_branch TEXT NOT NULL,
    target_branch TEXT NOT NULL,
    state         TEXT NOT NULL,
    merge_commit  TEXT,
    opened_at     TEXT NOT NULL
);
