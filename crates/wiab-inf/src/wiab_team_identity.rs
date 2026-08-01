use std::sync::Arc;

use wiab_app::{AccessApplicationService, IssueTokenRequest, TeamIdentity, UserApplicationService};
use wiab_core::access::{Role, Scope};
use wiab_core::organization::OrganizationId;
use wiab_core::team::TeamId;
use wiab_core::user::UserId;

use crate::{RoleAssignmentRepo, UserRepo};

/// Gives a team its own identity, by composing the user and access services.
///
/// This lives in infrastructure rather than in the team service because it spans two
/// aggregates the team does not own. It is the same sequence `create_agent` performs inline
/// for agents: mint a user, grant it Write on its org.
#[derive(Clone)]
pub struct WiabTeamIdentity {
    users: Arc<UserApplicationService<UserRepo>>,
    access: Arc<AccessApplicationService<RoleAssignmentRepo, UserRepo>>,
}

impl WiabTeamIdentity {
    pub fn new(
        users: Arc<UserApplicationService<UserRepo>>,
        access: Arc<AccessApplicationService<RoleAssignmentRepo, UserRepo>>,
    ) -> Self {
        Self { users, access }
    }
}

impl TeamIdentity for WiabTeamIdentity {
    async fn provision(
        &self,
        team_id: TeamId,
        name: &str,
        organization_id: OrganizationId,
    ) -> anyhow::Result<UserId> {
        let user = self
            .users
            .provision_team_user(name.to_owned(), team_id)
            .await?;
        let user_id: UserId = user.id.parse()?;
        // Write, not Administer: a team pushes branches and opens pull requests. It has no
        // business creating other agents or handing out roles.
        self.access
            .grant_direct(user_id, Scope::Org(organization_id), Role::Write)
            .await?;
        Ok(user_id)
    }

    async fn issue_token(
        &self,
        user_id: UserId,
        organization_id: OrganizationId,
    ) -> anyhow::Result<String> {
        let issued = self
            .users
            .issue_token(
                &user_id.to_string(),
                IssueTokenRequest {
                    label: format!("team sandbox {user_id}"),
                    read_only: false,
                    repos: None,
                    // Narrowed to the team's own org, so a leaked token cannot reach
                    // another organization's repos even though the user could not either.
                    orgs: Some(vec![organization_id.to_string()]),
                    expires_at: None,
                },
            )
            .await?
            .ok_or_else(|| anyhow::anyhow!("team user {user_id} has no record to issue against"))?;
        Ok(issued.plaintext)
    }

    async fn revoke_tokens(&self, user_id: UserId) -> anyhow::Result<()> {
        let revoked = self.users.revoke_all_tokens(&user_id.to_string()).await?;
        if let Some(count) = revoked
            && count > 0
        {
            tracing::info!("revoked {count} token(s) for team user {user_id}");
        }
        Ok(())
    }
}
