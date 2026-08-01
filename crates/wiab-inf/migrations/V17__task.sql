CREATE TABLE task (
    id       TEXT PRIMARY KEY,
    -- The numeric part of the id, kept as its own column so the board can be drained in
    -- arrival order: 'T-10' sorts before 'T-9' as text.
    number   BIGINT NOT NULL,
    version  BIGINT NOT NULL,
    board_id TEXT NOT NULL,
    work_id  TEXT NOT NULL,
    state    TEXT NOT NULL,
    assignee TEXT,
    reason   TEXT
);

-- claim_next scans one board for the oldest available task on every poll, so it is the one
-- query worth an index.
CREATE INDEX task_board_state_number_idx ON task (board_id, state, number);
