-- Organization aggregate root. `version` is the optimistic-concurrency token
-- (incremented on every save; guarded with `WHERE version = $expected`).
-- Ids are the canonical string form (e.g. "O-1"). No FK constraints: parent
-- existence is enforced in the application layer.
CREATE TABLE organization (
    id          TEXT PRIMARY KEY,
    version     BIGINT NOT NULL,
    name        TEXT NOT NULL,
    description TEXT NOT NULL
);
