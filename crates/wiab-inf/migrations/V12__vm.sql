CREATE TABLE vm (
    id              TEXT PRIMARY KEY,
    version         BIGINT NOT NULL,
    organization_id TEXT NOT NULL,
    template        TEXT NOT NULL,
    state           TEXT NOT NULL,
    guest_ip        TEXT,
    vcpus           BIGINT NOT NULL,
    mem_mib         BIGINT NOT NULL
);
