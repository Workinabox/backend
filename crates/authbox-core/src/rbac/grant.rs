use crate::rbac::{ResourceRef, Role};

/// A role held by a principal at a resource.
///
/// The principal is an opaque id string so the core stays decoupled from any product's
/// user-id type; a product stringifies its own id when building grants and matches on it
/// when querying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    principal: String,
    resource: ResourceRef,
    role: Role,
}

impl Grant {
    pub fn new(principal: impl Into<String>, resource: ResourceRef, role: Role) -> Self {
        Self {
            principal: principal.into(),
            resource,
            role,
        }
    }

    pub fn principal(&self) -> &str {
        &self.principal
    }

    pub fn resource(&self) -> &ResourceRef {
        &self.resource
    }

    pub fn role(&self) -> Role {
        self.role
    }
}
