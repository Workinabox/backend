CREATE TABLE project (
    id              TEXT PRIMARY KEY,
    version         BIGINT NOT NULL,
    organization_id TEXT NOT NULL,
    name            TEXT NOT NULL,
    description     TEXT NOT NULL
);
