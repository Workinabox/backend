CREATE TABLE board (
    id          TEXT PRIMARY KEY,
    version     BIGINT NOT NULL,
    project_id  TEXT NOT NULL,
    name        TEXT NOT NULL,
    description TEXT NOT NULL
);
