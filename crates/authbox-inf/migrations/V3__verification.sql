-- Single-use, expiring verification tokens (password reset, later: email verify / invites).
-- Only the hash of the secret is stored; the plaintext travels in the emailed link.
CREATE TABLE verification_token (
    token_hash   TEXT PRIMARY KEY,
    purpose      TEXT NOT NULL,
    principal_id TEXT NOT NULL,
    expires_at   TEXT NOT NULL
);

CREATE INDEX idx_verification_token_principal ON verification_token (principal_id);
