use crate::user::{
    AccessToken, ExternalRef, SshKey, SshKeyId, TokenId, UserError, UserId, UserKind, UserSnapshot,
    UserState,
};

/// A user: an identity that authenticates (human or agent) and owns its credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    id: UserId,
    kind: UserKind,
    name: String,
    email: Option<String>,
    state: UserState,
    external_refs: Vec<ExternalRef>,
    ssh_keys: Vec<SshKey>,
    tokens: Vec<AccessToken>,
}

impl User {
    pub fn new(
        id: UserId,
        kind: UserKind,
        name: String,
        email: Option<String>,
    ) -> Result<Self, UserError> {
        if name.trim().is_empty() {
            return Err(UserError::EmptyName);
        }
        Ok(Self {
            id,
            kind,
            name,
            email,
            state: UserState::Active,
            external_refs: Vec::new(),
            ssh_keys: Vec::new(),
            tokens: Vec::new(),
        })
    }

    /// Reconstitute a user from persisted state (used by repository implementations).
    /// Bypasses validation: the data was already validated when first created.
    #[allow(clippy::too_many_arguments)]
    pub fn from_persistence(
        id: UserId,
        kind: UserKind,
        name: String,
        email: Option<String>,
        state: UserState,
        external_refs: Vec<ExternalRef>,
        ssh_keys: Vec<SshKey>,
        tokens: Vec<AccessToken>,
    ) -> User {
        Self {
            id,
            kind,
            name,
            email,
            state,
            external_refs,
            ssh_keys,
            tokens,
        }
    }

    pub fn id(&self) -> UserId {
        self.id
    }

    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    pub fn ssh_keys(&self) -> &[SshKey] {
        &self.ssh_keys
    }

    pub fn tokens(&self) -> &[AccessToken] {
        &self.tokens
    }

    pub fn kind(&self) -> UserKind {
        self.kind
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn external_refs(&self) -> &[ExternalRef] {
        &self.external_refs
    }

    /// The id this user is linked to in `system`, if any (e.g. its agent id under `"agent"`).
    pub fn external_ref(&self, system: &str) -> Option<&str> {
        self.external_refs
            .iter()
            .find(|reference| reference.system() == system)
            .map(|reference| reference.id())
    }

    pub fn add_external_ref(&mut self, reference: ExternalRef) {
        self.external_refs.push(reference);
    }

    pub fn state(&self) -> UserState {
        self.state
    }

    /// Whether the user may authenticate. `Pending` (invited/unverified) and `Deactivated`
    /// users cannot.
    pub fn is_active(&self) -> bool {
        self.state == UserState::Active
    }

    /// Mark the user pending — invited but not accepted, or signed up but not yet verified.
    pub fn mark_pending(&mut self) {
        self.state = UserState::Pending;
    }

    pub fn activate(&mut self) {
        self.state = UserState::Active;
    }

    pub fn deactivate(&mut self) {
        self.state = UserState::Deactivated;
    }

    pub fn add_ssh_key(&mut self, key: SshKey) {
        self.ssh_keys.push(key);
    }

    pub fn remove_ssh_key(&mut self, id: &SshKeyId) -> Result<(), UserError> {
        let before = self.ssh_keys.len();
        self.ssh_keys.retain(|key| key.id() != *id);
        if self.ssh_keys.len() == before {
            return Err(UserError::SshKeyNotFound(id.to_string()));
        }
        Ok(())
    }

    /// The key whose fingerprint matches, used to resolve an SSH login to this user.
    pub fn ssh_key_by_fingerprint(&self, fingerprint: &str) -> Option<&SshKey> {
        self.ssh_keys
            .iter()
            .find(|key| key.fingerprint() == fingerprint)
    }

    pub fn add_token(&mut self, token: AccessToken) {
        self.tokens.push(token);
    }

    pub fn revoke_token(&mut self, id: &TokenId) -> Result<(), UserError> {
        let before = self.tokens.len();
        self.tokens.retain(|token| token.id() != *id);
        if self.tokens.len() == before {
            return Err(UserError::TokenNotFound(id.to_string()));
        }
        Ok(())
    }

    /// The token whose stored hash matches, used to resolve an HTTPS request to this user.
    pub fn token_by_hash(&self, hash: &str) -> Option<&AccessToken> {
        self.tokens.iter().find(|token| token.matches_hash(hash))
    }

    pub fn token_by_hash_mut(&mut self, hash: &str) -> Option<&mut AccessToken> {
        self.tokens
            .iter_mut()
            .find(|token| token.matches_hash(hash))
    }

    pub fn snapshot(&self) -> UserSnapshot {
        UserSnapshot {
            id: self.id.to_string(),
            kind: self.kind.to_string(),
            name: self.name.clone(),
            email: self.email.clone(),
            state: self.state.to_string(),
            agent_id: self.external_ref("agent").map(|id| id.to_owned()),
            ssh_keys: self.ssh_keys.iter().map(|key| key.snapshot()).collect(),
            tokens: self.tokens.iter().map(|token| token.snapshot()).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user::TokenScope;

    fn user() -> User {
        User::new(
            UserId::from_number(1),
            UserKind::Human,
            "Ada".to_owned(),
            Some("ada@example.com".to_owned()),
        )
        .unwrap()
    }

    fn key(label: &str, fingerprint: &str) -> SshKey {
        SshKey::new(
            SshKeyId::new(),
            label.to_owned(),
            "ssh-ed25519 AAAA...".to_owned(),
            fingerprint.to_owned(),
        )
        .unwrap()
    }

    fn token(hash: &str) -> AccessToken {
        AccessToken::new(
            TokenId::new(),
            "ci".to_owned(),
            hash.to_owned(),
            "wiab_pat_…1234".to_owned(),
            "2026-01-01T00:00:00Z".to_owned(),
            None,
            TokenScope::unrestricted(),
        )
        .unwrap()
    }

    #[test]
    fn rejects_empty_name() {
        assert_eq!(
            User::new(
                UserId::from_number(1),
                UserKind::Human,
                "  ".to_owned(),
                None
            )
            .unwrap_err(),
            UserError::EmptyName
        );
    }

    #[test]
    fn lifecycle_transitions_gate_activity() {
        let mut user = user();
        assert!(user.is_active());
        assert_eq!(user.snapshot().state, "active");
        user.mark_pending();
        assert!(!user.is_active());
        assert_eq!(user.snapshot().state, "pending");
        user.deactivate();
        assert!(!user.is_active());
        user.activate();
        assert!(user.is_active());
    }

    #[test]
    fn exposes_identity_fields() {
        let human = user();
        assert_eq!(human.id(), UserId::from_number(1));
        assert_eq!(human.kind(), UserKind::Human);
        assert_eq!(human.name(), "Ada");
        assert!(human.external_ref("agent").is_none());

        let mut agent = User::new(
            UserId::from_number(2),
            UserKind::Agent,
            "bot".to_owned(),
            None,
        )
        .unwrap();
        agent.add_external_ref(ExternalRef::new("agent", "A-9"));
        assert_eq!(agent.kind(), UserKind::Agent);
        assert_eq!(agent.external_ref("agent"), Some("A-9"));
        assert_eq!(agent.snapshot().agent_id.as_deref(), Some("A-9"));
    }

    #[test]
    fn token_by_hash_mut_marks_use() {
        let mut user = user();
        user.add_token(token("h"));
        user.token_by_hash_mut("h")
            .unwrap()
            .mark_used("2026-06-13T00:00:00Z".to_owned());
        assert!(user.snapshot().tokens[0].last_used_at.is_some());
        assert!(user.token_by_hash_mut("nope").is_none());
    }

    #[test]
    fn resolves_login_by_key_fingerprint() {
        let mut user = user();
        let key = key("laptop", "SHA256:abc");
        let id = key.id();
        user.add_ssh_key(key);
        assert_eq!(user.ssh_key_by_fingerprint("SHA256:abc").unwrap().id(), id);
        assert!(user.ssh_key_by_fingerprint("SHA256:zzz").is_none());
        user.remove_ssh_key(&id).unwrap();
        assert!(user.ssh_key_by_fingerprint("SHA256:abc").is_none());
        assert!(user.remove_ssh_key(&id).is_err());
    }

    #[test]
    fn resolves_request_by_token_hash() {
        let mut user = user();
        let token = token("hash-1");
        let id = token.id();
        user.add_token(token);
        assert_eq!(user.token_by_hash("hash-1").unwrap().id(), id);
        assert!(user.token_by_hash("nope").is_none());
        user.revoke_token(&id).unwrap();
        assert!(user.token_by_hash("hash-1").is_none());
        assert!(user.revoke_token(&id).is_err());
    }

    #[test]
    fn snapshot_excludes_secrets() {
        let mut user = user();
        user.add_token(token("secret-hash"));
        let snapshot = user.snapshot();
        assert_eq!(snapshot.id, "U-1");
        assert_eq!(snapshot.kind, "human");
        assert_eq!(snapshot.tokens.len(), 1);
        // The hash must never appear in the snapshot.
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(!json.contains("secret-hash"));
    }

    #[test]
    fn from_persistence_round_trips_with_credentials_and_some_fields() {
        let key = key("laptop", "SHA256:abc");
        let key_id = key.id();
        let token = token("hash-1");
        let token_id = token.id();
        let user = User::from_persistence(
            UserId::from_number(4),
            UserKind::Agent,
            "bot".to_owned(),
            Some("bot@example.com".to_owned()),
            UserState::Active,
            vec![ExternalRef::new("agent", "A-9")],
            vec![key],
            vec![token],
        );
        assert_eq!(user.id(), UserId::from_number(4));
        assert_eq!(user.kind(), UserKind::Agent);
        assert_eq!(user.name(), "bot");
        assert_eq!(user.email(), Some("bot@example.com"));
        assert_eq!(user.external_ref("agent"), Some("A-9"));
        assert_eq!(user.ssh_keys().len(), 1);
        assert_eq!(user.ssh_keys()[0].id(), key_id);
        assert_eq!(user.tokens().len(), 1);
        assert_eq!(user.tokens()[0].id(), token_id);
    }

    #[test]
    fn from_persistence_round_trips_with_none_fields() {
        let user = User::from_persistence(
            UserId::from_number(5),
            UserKind::Human,
            "Ada".to_owned(),
            None,
            UserState::Active,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        assert!(user.email().is_none());
        assert!(user.external_refs().is_empty());
        assert!(user.ssh_keys().is_empty());
        assert!(user.tokens().is_empty());
    }
}
