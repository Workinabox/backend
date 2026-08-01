-- Events are written here in the same transaction as the aggregate they describe, so an
-- event cannot survive a rolled-back change nor be lost after a committed one. A
-- background publisher drains the table and deletes what it has sent.
CREATE TABLE outbox (
    id            BIGSERIAL PRIMARY KEY,
    name          TEXT NOT NULL,
    aggregate_id  TEXT NOT NULL,
    payload       JSONB NOT NULL,
    -- Publishing goes in insertion order, which is the order things happened.
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX outbox_id_idx ON outbox (id);
