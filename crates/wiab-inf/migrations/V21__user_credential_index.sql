-- Every authenticated request resolves a presented credential to its owner: a bearer/basic
-- token by hash, an SSH key by fingerprint. Without these the lookup scans every user's
-- credentials, so authentication cost grows with the size of the user table.

-- Deliberately not UNIQUE. Nothing rejects a duplicate SSH key today, so two users may already
-- share one; a unique index would then fail to create, and since migrations run at boot the
-- process would refuse to start. Deciding who owns a shared key is a domain question, not a
-- side effect of adding an index. Token hashes are 256-bit CSPRNG values and collide only in
-- theory, but the same reasoning applies and a plain index performs identically here.
CREATE INDEX idx_user_access_token_hash ON user_access_token (hash);
CREATE INDEX idx_user_ssh_key_fingerprint ON user_ssh_key (fingerprint);
