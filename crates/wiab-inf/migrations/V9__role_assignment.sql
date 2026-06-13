-- Role-assignment (access grant) aggregate root. `version` is the optimistic-concurrency
-- token (incremented on every save; guarded with `WHERE version = $expected`).
-- `scope` is denormalised into `scope_kind` ("org"/"project"/"repo") and `scope_id`
-- (the canonical id string). Ids are the canonical string form (e.g. "G-1", "U-1").
-- No FK constraints: parent existence is enforced in the application layer.
CREATE TABLE role_assignment (
    id         TEXT PRIMARY KEY,
    version    BIGINT NOT NULL,
    user_id    TEXT NOT NULL,
    scope_kind TEXT NOT NULL,
    scope_id   TEXT NOT NULL,
    role       TEXT NOT NULL
);
