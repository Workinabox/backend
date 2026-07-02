use std::fmt;

use crate::vm::VmError;

/// The template a microVM boots from, identified by name (e.g. "base", "developer").
///
/// A template is the "base extended per role" unit: `base` is the common headless image and
/// each role (e.g. `developer`) is a child image layered on it. The domain only carries the
/// validated name; resolving the name to a concrete rootfs/kernel is an infrastructure concern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmTemplate(String);

impl VmTemplate {
    pub fn new(name: impl Into<String>) -> Result<Self, VmError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(VmError::EmptyTemplate);
        }
        Ok(Self(name))
    }

    pub fn name(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VmTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_name() {
        assert_eq!(VmTemplate::new("developer").unwrap().name(), "developer");
    }

    #[test]
    fn rejects_empty_name() {
        assert_eq!(VmTemplate::new("  ").unwrap_err(), VmError::EmptyTemplate);
    }
}
