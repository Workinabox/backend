use crate::auth::PrincipalId;

/// A link between a host principal and an external identity at an OIDC issuer. Resolution
/// keys on `(issuer, subject)` — `subject` is the provider's stable id, never the email.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FederatedIdentity {
    principal: PrincipalId,
    issuer: String,
    subject: String,
    email: Option<String>,
    linked_at: String,
}

impl FederatedIdentity {
    pub fn new(
        principal: PrincipalId,
        issuer: String,
        subject: String,
        email: Option<String>,
        linked_at: String,
    ) -> Self {
        Self {
            principal,
            issuer,
            subject,
            email,
            linked_at,
        }
    }

    pub fn principal(&self) -> &PrincipalId {
        &self.principal
    }

    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    pub fn linked_at(&self) -> &str {
        &self.linked_at
    }
}
