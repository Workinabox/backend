-- Generic external-system references for a user (e.g. ("agent","A-9"), or a future SCIM
-- externalId), replacing the WIAB-specific `app_user.agent_id` column. Ordered by
-- `position` within the owning user, like the other owned child tables.
CREATE TABLE user_external_ref (
    user_id  TEXT NOT NULL,
    position INTEGER NOT NULL,
    system   TEXT NOT NULL,
    ref_id   TEXT NOT NULL,
    PRIMARY KEY (user_id, position)
);

-- Carry forward existing agent links, then drop the column they came from.
INSERT INTO user_external_ref (user_id, position, system, ref_id)
    SELECT id, 0, 'agent', agent_id FROM app_user WHERE agent_id IS NOT NULL;

ALTER TABLE app_user DROP COLUMN agent_id;
