use crate::rbac::Role;

/// An action a request wants to perform on a resource, mapped to the minimum role it needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    /// Clone / fetch / browse.
    Read,
    /// Push / commit.
    Write,
    /// Change resource settings (e.g. visibility).
    Administer,
    /// Manage the owning scope and its members.
    Own,
}

impl Operation {
    pub fn required_role(&self) -> Role {
        match self {
            Operation::Read => Role::Read,
            Operation::Write => Role::Write,
            Operation::Administer => Role::Admin,
            Operation::Own => Role::Owner,
        }
    }
}
