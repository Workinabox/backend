-- Federated-identity links: a host principal ↔ an external (issuer, subject). Resolution
-- and uniqueness are on (issuer, subject); the subject is the IdP's stable id, not email.
CREATE TABLE federated_identity (
    issuer       TEXT NOT NULL,
    subject      TEXT NOT NULL,
    principal_id TEXT NOT NULL,
    email        TEXT,
    linked_at    TEXT NOT NULL,
    PRIMARY KEY (issuer, subject)
);

CREATE INDEX idx_federated_identity_principal ON federated_identity (principal_id);

-- Short-lived OIDC login state (PKCE verifier, nonce, return target), single-use by `state`.
CREATE TABLE auth_flow (
    state         TEXT PRIMARY KEY,
    connection    TEXT NOT NULL,
    nonce         TEXT NOT NULL,
    pkce_verifier TEXT NOT NULL,
    return_to     TEXT NOT NULL,
    expires_at    TEXT NOT NULL
);
