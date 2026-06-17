use std::fmt;

/// An opaque reference to a host user ("principal").
///
/// The auth layer keys all its state (sessions, password credentials, federated
/// identities) on this and never interprets it; the host (e.g. WIAB) maps it to its own
/// user id (`"U-1"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PrincipalId(String);

impl PrincipalId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PrincipalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
