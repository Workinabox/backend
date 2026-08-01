use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use wiab_core::repository::{RepoError, SaveError, Version};
use wiab_core::team::{Team, TeamId, TeamRepository};

#[derive(Debug, Clone, Default)]
pub struct InMemoryTeamRepository {
    teams: Arc<RwLock<HashMap<TeamId, (Team, u64)>>>,
}

impl InMemoryTeamRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TeamRepository for InMemoryTeamRepository {
    async fn save(&self, team: Team, expected: Version) -> Result<Version, SaveError> {
        let mut teams = self
            .teams
            .write()
            .expect("team repository write lock poisoned");
        let current = teams
            .get(&team.id())
            .map(|(_, version)| *version)
            .unwrap_or(0);
        if current != expected.value() {
            return Err(SaveError::Conflict);
        }
        let next = expected.next();
        teams.insert(team.id(), (team, next.value()));
        Ok(next)
    }

    async fn get(&self, id: &TeamId) -> Result<Option<(Team, Version)>, RepoError> {
        Ok(self
            .teams
            .read()
            .expect("team repository read lock poisoned")
            .get(id)
            .map(|(team, version)| (team.clone(), Version::from_value(*version))))
    }

    async fn list(&self) -> Result<Vec<Team>, RepoError> {
        Ok(self
            .teams
            .read()
            .expect("team repository read lock poisoned")
            .values()
            .map(|(team, _)| team.clone())
            .collect())
    }
}
