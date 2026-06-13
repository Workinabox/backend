-- Work aggregate root. `version` is the optimistic-concurrency token
-- (incremented on every save; guarded with `WHERE version = $expected`).
-- Ids are the canonical string form (e.g. "W-7", "P-1"). No FK constraints:
-- parent existence is enforced in the application layer.
CREATE TABLE work (
    id          TEXT PRIMARY KEY,
    version     BIGINT NOT NULL,
    project_id  TEXT NOT NULL,
    title       TEXT NOT NULL,
    description TEXT NOT NULL
);

-- Child table for the `dones` collection owned by a `work`. Rows are rewritten
-- wholesale on every save and ordered by `position`.
CREATE TABLE work_done (
    work_id   TEXT NOT NULL,
    position  INTEGER NOT NULL,
    done_id   TEXT NOT NULL,
    criterion TEXT NOT NULL,
    fulfilled BOOLEAN NOT NULL,
    PRIMARY KEY (work_id, position)
);
