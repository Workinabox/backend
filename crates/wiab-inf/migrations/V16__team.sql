CREATE TABLE team (
    id              TEXT PRIMARY KEY,
    version         BIGINT NOT NULL,
    organization_id TEXT NOT NULL,
    name            TEXT NOT NULL,
    description     TEXT NOT NULL,
    vm_template     TEXT NOT NULL,
    state           TEXT NOT NULL,
    vm_id           TEXT
);
