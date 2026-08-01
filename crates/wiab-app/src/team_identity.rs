use wiab_core::organization::OrganizationId;
use wiab_core::team::TeamId;
use wiab_core::user::UserId;

/// How a team gets, and proves, its own identity.
///
/// A team authenticates to the backend to claim work and to push, so it needs a credential.
/// Rather than sharing one across every team, each gets its own user — the arrangement
/// `create_agent` already uses for agents — so what a team may do is an ordinary access
/// grant and its actions are attributable to it.
///
/// A narrow port because provisioning spans two aggregates the team service does not own
/// (`User` and `RoleAssignment`), and because it lets the service be tested without either.
#[allow(async_fn_in_trait)]
pub trait TeamIdentity: Send + Sync {
    /// Create the team's user and grant it Write on its organization.
    async fn provision(
        &self,
        team_id: TeamId,
        name: &str,
        organization_id: OrganizationId,
    ) -> anyhow::Result<UserId>;

    /// Mint a fresh access token, returning the one-time plaintext.
    ///
    /// Called on every start, and the plaintext goes straight into the container's
    /// environment — so nothing has to store a secret at rest, and a token only ever lives
    /// as long as the container it was minted for.
    async fn issue_token(
        &self,
        user_id: UserId,
        organization_id: OrganizationId,
    ) -> anyhow::Result<String>;

    /// Revoke every token the team holds.
    ///
    /// Called when a team stops. A token is minted per start and only ever reaches that
    /// container, so once the container is gone the token has no legitimate user left —
    /// leaving it valid is a credential outliving its purpose for no reason.
    async fn revoke_tokens(&self, user_id: UserId) -> anyhow::Result<()>;
}
