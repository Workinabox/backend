use std::sync::Arc;

use authbox_core::auth::{AuthError, PrincipalId, UserDirectory};
use wiab_app::{CreateUserRequest, UserApplicationService};

use crate::UserRepo;

/// Bridges the auth layer's [`UserDirectory`] port to WIAB's user store: resolves a login
/// email to a principal (the user's `U-…` id) via the user application service. This is the
/// seam that keeps `authbox` decoupled from WIAB's concrete `User` type.
#[derive(Clone)]
pub struct WiabUserDirectory {
    user_service: Arc<UserApplicationService<UserRepo>>,
}

impl WiabUserDirectory {
    pub fn new(user_service: Arc<UserApplicationService<UserRepo>>) -> Self {
        Self { user_service }
    }
}

impl UserDirectory for WiabUserDirectory {
    async fn find_by_email(&self, email: &str) -> Result<Option<PrincipalId>, AuthError> {
        let user = self
            .user_service
            .find_by_email(email)
            .await
            .map_err(|error| AuthError::Backend(error.to_string()))?;
        Ok(user.map(|id| PrincipalId::new(id.to_string())))
    }

    async fn provision(&self, email: &str, name: &str) -> Result<PrincipalId, AuthError> {
        let snapshot = self
            .user_service
            .create_user(CreateUserRequest {
                kind: "human".to_owned(),
                name: name.to_owned(),
                email: Some(email.to_owned()),
            })
            .await
            .map_err(|error| AuthError::Backend(error.to_string()))?;
        Ok(PrincipalId::new(snapshot.id))
    }
}
