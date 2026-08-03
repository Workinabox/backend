use crate::auth::AuthError;

/// Shortest password accepted.
pub const MIN_PASSWORD_LENGTH: usize = 8;

/// Longest password accepted.
///
/// Not a strength rule — a bound on work. Argon2id is configured at ~19 MiB here and its cost
/// scales with the input, so an unbounded password field lets an anonymous request buy
/// arbitrary CPU. 128 is far past any real passphrase.
pub const MAX_PASSWORD_LENGTH: usize = 128;

/// The single definition of what counts as an acceptable password.
///
/// It lives in core, and the application services call it, because the rule was previously
/// duplicated as `len() < 8` in four HTTP handlers: the services themselves accepted anything,
/// so any path that did not go through a handler had no policy at all.
pub fn validate_password(plaintext: &str) -> Result<(), AuthError> {
    if plaintext.len() < MIN_PASSWORD_LENGTH || plaintext.len() > MAX_PASSWORD_LENGTH {
        return Err(AuthError::PasswordPolicy {
            min: MIN_PASSWORD_LENGTH,
            max: MAX_PASSWORD_LENGTH,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bounds_are_inclusive() {
        assert!(validate_password(&"a".repeat(MIN_PASSWORD_LENGTH)).is_ok());
        assert!(validate_password(&"a".repeat(MAX_PASSWORD_LENGTH)).is_ok());
        assert!(validate_password(&"a".repeat(MIN_PASSWORD_LENGTH - 1)).is_err());
        assert!(validate_password(&"a".repeat(MAX_PASSWORD_LENGTH + 1)).is_err());
    }

    #[test]
    fn an_empty_password_is_refused() {
        // The services used to accept this: only the handlers checked, and not all callers
        // are handlers.
        assert!(validate_password("").is_err());
    }
}
