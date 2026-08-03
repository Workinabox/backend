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
    /// Accepts a name only if it is `[a-z0-9][a-z0-9._-]*`.
    ///
    /// This is the one choke point between a request body and two runtimes that both build a
    /// resource identifier out of the name, so the character set is a domain invariant rather
    /// than either adapter's business:
    ///
    /// - Firecracker resolves it to `<images_dir>/<template>.ext4`, so a `/` or `..` escapes the
    ///   image directory and mounts an arbitrary host file into a guest.
    /// - Docker interpolates it into `<prefix><template>:<tag>`, so a `/`, `:` or `@` redirects
    ///   which image is pulled and run — `evil.registry.com/x` is a whole other registry.
    ///
    /// An allow-list rather than a deny-list: the set of legal template names is small and
    /// known, and there is no reason to reason about every metacharacter two different
    /// identifier grammars might treat as special.
    pub fn new(name: impl Into<String>) -> Result<Self, VmError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(VmError::EmptyTemplate);
        }
        if !is_valid_template_name(&name) {
            return Err(VmError::InvalidTemplate(name));
        }
        Ok(Self(name))
    }

    pub fn name(&self) -> &str {
        &self.0
    }
}

/// Hand-rolled rather than a regex crate: the rule is four `matches!` arms and the workspace
/// has no regex dependency to justify adding for this.
fn is_valid_template_name(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    characters.all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '.' | '_' | '-')
    })
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

    #[test]
    fn accepts_the_names_real_templates_use() {
        for name in ["base", "developer", "base-2", "python3.12", "team_worker"] {
            assert!(VmTemplate::new(name).is_ok(), "{name} should be accepted");
        }
    }

    /// The two sinks this guards: a filesystem path (Firecracker) and an image reference
    /// (Docker). Both are built by interpolating the name, so anything that means something in
    /// either grammar has to be refused here.
    #[test]
    fn rejects_names_that_escape_a_path_or_redirect_an_image() {
        for hostile in [
            "../../etc/passwd",    // escapes images_dir
            "..",                  //
            "base/../../root",     //
            "/etc/shadow",         // absolute path
            "evil.registry.com/x", // another registry
            "base:latest",         // pins a different tag
            "base@sha256:abc",     // pins a different digest
            "base latest",         // whitespace
            "Base",                // uppercase: not a legal image name either
            "-base",               // leading dash
            ".base",               // leading dot
            "base;rm -rf /",       //
            "base\nX-Injected: 1", //
        ] {
            assert_eq!(
                VmTemplate::new(hostile).unwrap_err(),
                VmError::InvalidTemplate(hostile.to_owned()),
                "{hostile:?} should be rejected"
            );
        }
    }
}
