/// An opaque reference to a protected resource: a `(kind, id)` pair.
///
/// The core treats both halves as untyped strings so it stays decoupled from any
/// product's resource types. A product maps its own scopes onto this (WIAB renders
/// `("org", "O-1")`, `("repo", "R-3")`, …) and recovers them at its boundary. The shape
/// matches a role assignment's `(scope_kind, scope_id)` columns 1:1.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceRef {
    kind: String,
    id: String,
}

impl ResourceRef {
    pub fn new(kind: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            id: id.into(),
        }
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}
