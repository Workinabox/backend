use std::sync::Arc;

use anyhow::anyhow;
use wiab_core::board::{BoardId, BoardRepository};
use wiab_core::organization::{OrganizationId, OrganizationRepository};
use wiab_core::repo::{RepoId, RepoRepository};
use wiab_core::repository::{SaveError, Version};
use wiab_core::team::{Team, TeamId, TeamNumbering, TeamRepository, TeamSnapshot};
use wiab_core::user::UserId;
use wiab_core::vm::{VmId, VmTemplate};

use crate::team_identity::TeamIdentity;
use crate::team_requests::CreateTeamRequest;
use crate::vm_provisioning::VmProvisioning;
use crate::vm_requests::ProvisionVmRequest;

/// Everything the team's container needs to find its own work.
///
/// The names are the ones `wiab-team work` reads (see `sw-dev-team`'s `config.py`); the clone
/// URL is the same `<api>/repos/R-<n>.git` the git transport serves, so the repo id and the
/// remote never drift apart.
///
/// The token is minted fresh for this container and never stored, so a credential lives only
/// as long as the container it was issued to.
fn worker_env(team: &TeamSnapshot, api_url: &str, token: &str) -> Vec<(String, String)> {
    let api_url = api_url.trim_end_matches('/');
    vec![
        ("WIAB_TEAM_API_URL".to_owned(), api_url.to_owned()),
        ("WIAB_TEAM_TEAM_ID".to_owned(), team.id.clone()),
        ("WIAB_TEAM_BOARD_ID".to_owned(), team.board_id.clone()),
        (
            "WIAB_TEAM_REPO_REMOTE".to_owned(),
            format!("{api_url}/repos/{}.git", team.repo_id),
        ),
        ("WIAB_TEAM_API_TOKEN".to_owned(), token.to_owned()),
    ]
}

/// Orchestrates use cases over the `Team` aggregate — creation, and the start/pause/resume/stop
/// lifecycle. Starting provisions a sandbox through the [`VmProvisioning`] port (the same port
/// agent activation uses) and records the VM on the team; stopping releases it. Pausing does
/// not: a paused team keeps its container so resuming needs no re-provisioning.
///
/// Mutations use optimistic concurrency: load with version, apply, retry on conflict. Holds the
/// organization, board and repo repositories to verify everything a team references exists
/// before it is created — a team pointing at a board that was never written would start, poll
/// nothing, and look idle rather than broken.
pub struct TeamApplicationService<
    T: TeamRepository,
    O: OrganizationRepository,
    B: BoardRepository,
    R: RepoRepository,
    V: VmProvisioning,
    I: TeamIdentity,
> {
    team_repository: T,
    organization_repository: O,
    board_repository: B,
    repo_repository: R,
    vm: V,
    identity: I,
    numbering: Arc<dyn TeamNumbering>,
    /// Where the team's container reaches this backend. The container polls over HTTP, so it
    /// needs the public URL rather than the address the server happens to bind.
    api_url: String,
}

impl<
    T: TeamRepository,
    O: OrganizationRepository,
    B: BoardRepository,
    R: RepoRepository,
    V: VmProvisioning,
    I: TeamIdentity,
> TeamApplicationService<T, O, B, R, V, I>
{
    /// One argument per collaborator. A builder would hide which ones are required, which is
    /// the only thing worth knowing here.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        team_repository: T,
        organization_repository: O,
        board_repository: B,
        repo_repository: R,
        vm: V,
        identity: I,
        numbering: Arc<dyn TeamNumbering>,
        api_url: String,
    ) -> Self {
        Self {
            team_repository,
            organization_repository,
            board_repository,
            repo_repository,
            vm,
            identity,
            numbering,
            api_url,
        }
    }

    /// Returns `Ok(None)` when no organization with the given id exists.
    pub async fn list_teams(
        &self,
        organization_id: &str,
    ) -> anyhow::Result<Option<Vec<TeamSnapshot>>> {
        let id: OrganizationId = organization_id.parse()?;
        if self.organization_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        let mut teams = self
            .team_repository
            .list()
            .await?
            .into_iter()
            .filter(|team| team.organization_id() == id)
            .collect::<Vec<_>>();
        teams.sort_by_key(|team| team.id().number());
        Ok(Some(teams.iter().map(Team::snapshot).collect()))
    }

    pub async fn team_snapshot(&self, team_id: &str) -> anyhow::Result<Option<TeamSnapshot>> {
        let id: TeamId = team_id.parse()?;
        Ok(self
            .team_repository
            .get(&id)
            .await?
            .map(|(team, _)| team.snapshot()))
    }

    /// Returns `Ok(None)` when the organization, the board or the repo is unknown.
    pub async fn create_team(
        &self,
        organization_id: &str,
        request: CreateTeamRequest,
    ) -> anyhow::Result<Option<TeamSnapshot>> {
        let id: OrganizationId = organization_id.parse()?;
        let board_id: BoardId = request.board_id.parse()?;
        let repo_id: RepoId = request.repo_id.parse()?;
        if self.organization_repository.get(&id).await?.is_none()
            || self.board_repository.get(&board_id).await?.is_none()
            || self.repo_repository.get(&repo_id).await?.is_none()
        {
            return Ok(None);
        }
        let template = VmTemplate::new(request.vm_type)?;
        let team_id = self.numbering.next();
        // Provisioned before the aggregate exists, so a team is never persisted without the
        // identity it needs in order to do anything.
        let user_id = self.identity.provision(team_id, &request.name, id).await?;
        let team = Team::new(
            team_id,
            id,
            request.name,
            request.description,
            board_id,
            repo_id,
            user_id,
            template,
        )?;
        let snapshot = team.snapshot();
        self.team_repository.save(team, Version::NEW).await?;
        Ok(Some(snapshot))
    }

    /// Start the team: mark it `Starting`, provision its sandbox, then mark it `Idle`.
    ///
    /// The two-step recording is deliberate — `Starting` is persisted *before* provisioning so a
    /// concurrent start is rejected by the aggregate rather than booting a second container. If
    /// provisioning fails the team is left `Failed`, not silently `Stopped`.
    ///
    /// `Ok(None)` when no team with the given id exists.
    pub async fn start_team(&self, team_id: &str) -> anyhow::Result<Option<TeamSnapshot>> {
        let id: TeamId = team_id.parse()?;
        let Some(team) = self.mutate(&id, |team| team.start()).await? else {
            return Ok(None);
        };
        let user_id: UserId = team.user_id.parse()?;
        let organization: OrganizationId = team.organization_id.parse()?;
        let token = self.identity.issue_token(user_id, organization).await?;
        let request = ProvisionVmRequest {
            template: team.vm_template.clone(),
            vcpus: None,
            mem_mib: None,
            env: worker_env(&team, &self.api_url, &token),
        };
        let vm = match self
            .vm
            .provision(&team.organization_id, team_id, request)
            .await
        {
            Ok(Some(vm)) => vm,
            outcome => {
                self.mutate(&id, |team| {
                    team.mark_failed();
                    Ok(())
                })
                .await?;
                return match outcome {
                    Err(error) => Err(error),
                    _ => Err(anyhow!("could not provision a sandbox for team {team_id}")),
                };
            }
        };
        let vm_id: VmId = vm.id.parse()?;
        self.mutate(&id, |team| team.mark_idle(vm_id)).await
    }

    /// Stop taking new work but keep the container. `Ok(None)` when no such team exists.
    pub async fn pause_team(&self, team_id: &str) -> anyhow::Result<Option<TeamSnapshot>> {
        let id: TeamId = team_id.parse()?;
        self.mutate(&id, |team| team.pause()).await
    }

    /// Take work again, on the container the team never released. `Ok(None)` when no such team.
    pub async fn resume_team(&self, team_id: &str) -> anyhow::Result<Option<TeamSnapshot>> {
        let id: TeamId = team_id.parse()?;
        self.mutate(&id, |team| team.resume()).await
    }

    /// Tear the team down, stopping its sandbox. `Ok(None)` when no such team exists.
    pub async fn stop_team(&self, team_id: &str) -> anyhow::Result<Option<TeamSnapshot>> {
        let id: TeamId = team_id.parse()?;
        let Some((team, _)) = self.team_repository.get(&id).await? else {
            return Ok(None);
        };
        if let Some(vm_id) = team.vm_id() {
            self.vm.stop(&vm_id.to_string()).await?;
        }
        self.mutate(&id, |team| {
            team.stop();
            Ok(())
        })
        .await
    }

    /// Load, apply a transition, save — retrying the whole cycle if another writer got there
    /// first. `Ok(None)` when no team with the given id exists.
    async fn mutate(
        &self,
        id: &TeamId,
        transition: impl Fn(&mut Team) -> Result<(), wiab_core::team::TeamError>,
    ) -> anyhow::Result<Option<TeamSnapshot>> {
        loop {
            let Some((mut team, version)) = self.team_repository.get(id).await? else {
                return Ok(None);
            };
            transition(&mut team)?;
            let snapshot = team.snapshot();
            match self.team_repository.save(team, version).await {
                Ok(_) => return Ok(Some(snapshot)),
                Err(SaveError::Conflict) => continue,
                Err(SaveError::Backend(error)) => return Err(anyhow!(error)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::RwLock;
    use std::sync::atomic::{AtomicU64, Ordering};

    use wiab_core::board::Board;
    use wiab_core::organization::Organization;
    use wiab_core::project::ProjectId;
    use wiab_core::repo::{Repo, Visibility};
    use wiab_core::repository::RepoError;
    use wiab_core::team::TeamState;
    use wiab_core::vm::VmSnapshot;

    use super::*;

    #[derive(Default)]
    struct TestTeamRepository {
        teams: RwLock<HashMap<TeamId, (Team, u64)>>,
    }

    impl TeamRepository for TestTeamRepository {
        async fn save(&self, team: Team, expected: Version) -> Result<Version, SaveError> {
            let mut teams = self.teams.write().expect("test write lock poisoned");
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
                .expect("test read lock poisoned")
                .get(id)
                .map(|(team, version)| (team.clone(), Version::from_value(*version))))
        }

        async fn list(&self) -> Result<Vec<Team>, RepoError> {
            Ok(self
                .teams
                .read()
                .expect("test read lock poisoned")
                .values()
                .map(|(team, _)| team.clone())
                .collect())
        }
    }

    #[derive(Default)]
    struct TestOrganizationRepository {
        organizations: RwLock<HashMap<OrganizationId, (Organization, u64)>>,
    }

    impl OrganizationRepository for TestOrganizationRepository {
        async fn save(
            &self,
            organization: Organization,
            expected: Version,
        ) -> Result<Version, SaveError> {
            let mut organizations = self
                .organizations
                .write()
                .expect("test write lock poisoned");
            let next = expected.next();
            organizations.insert(organization.id(), (organization, next.value()));
            Ok(next)
        }

        async fn get(
            &self,
            id: &OrganizationId,
        ) -> Result<Option<(Organization, Version)>, RepoError> {
            Ok(self
                .organizations
                .read()
                .expect("test read lock poisoned")
                .get(id)
                .map(|(organization, version)| {
                    (organization.clone(), Version::from_value(*version))
                }))
        }

        async fn list(&self) -> Result<Vec<Organization>, RepoError> {
            Ok(self
                .organizations
                .read()
                .expect("test read lock poisoned")
                .values()
                .map(|(organization, _)| organization.clone())
                .collect())
        }
    }

    /// Answers `get` for every id it was seeded with. The team service only ever asks
    /// whether a board or repo exists, so nothing more is needed.
    #[derive(Default)]
    struct TestBoardRepository {
        boards: RwLock<HashMap<BoardId, Board>>,
    }

    impl BoardRepository for TestBoardRepository {
        async fn save(&self, board: Board, expected: Version) -> Result<Version, SaveError> {
            self.boards
                .write()
                .expect("test write lock poisoned")
                .insert(board.id(), board);
            Ok(expected.next())
        }

        async fn get(&self, id: &BoardId) -> Result<Option<(Board, Version)>, RepoError> {
            Ok(self
                .boards
                .read()
                .expect("test read lock poisoned")
                .get(id)
                .map(|board| (board.clone(), Version::from_value(1))))
        }

        async fn list(&self) -> Result<Vec<Board>, RepoError> {
            Ok(self
                .boards
                .read()
                .expect("test read lock poisoned")
                .values()
                .cloned()
                .collect())
        }
    }

    #[derive(Default)]
    struct TestRepoRepository {
        repos: RwLock<HashMap<RepoId, Repo>>,
    }

    impl RepoRepository for TestRepoRepository {
        async fn save(&self, repo: Repo, expected: Version) -> Result<Version, SaveError> {
            self.repos
                .write()
                .expect("test write lock poisoned")
                .insert(repo.id(), repo);
            Ok(expected.next())
        }

        async fn get(&self, id: &RepoId) -> Result<Option<(Repo, Version)>, RepoError> {
            Ok(self
                .repos
                .read()
                .expect("test read lock poisoned")
                .get(id)
                .map(|repo| (repo.clone(), Version::from_value(1))))
        }

        async fn list(&self) -> Result<Vec<Repo>, RepoError> {
            Ok(self
                .repos
                .read()
                .expect("test read lock poisoned")
                .values()
                .cloned()
                .collect())
        }
    }

    /// Hands out a fixed user and a counted token, so a test can see that each start mints
    /// a new one rather than reusing the last.
    #[derive(Default)]
    struct StubTeamIdentity {
        tokens: RwLock<Vec<String>>,
        granted: RwLock<Vec<(String, String)>>,
    }

    impl TeamIdentity for StubTeamIdentity {
        async fn provision(
            &self,
            team_id: TeamId,
            name: &str,
            organization_id: OrganizationId,
        ) -> anyhow::Result<UserId> {
            self.granted
                .write()
                .expect("test write lock poisoned")
                .push((team_id.to_string(), format!("{name}@{organization_id}")));
            Ok(UserId::from_number(9))
        }

        async fn issue_token(
            &self,
            _user_id: UserId,
            _organization_id: OrganizationId,
        ) -> anyhow::Result<String> {
            let mut tokens = self.tokens.write().expect("test write lock poisoned");
            let token = format!("tok-{}", tokens.len() + 1);
            tokens.push(token.clone());
            Ok(token)
        }
    }

    #[derive(Default)]
    struct TestTeamNumbering {
        counter: AtomicU64,
    }

    impl TeamNumbering for TestTeamNumbering {
        fn next(&self) -> TeamId {
            TeamId::from_number(self.counter.fetch_add(1, Ordering::SeqCst) + 1)
        }
    }

    fn stub_vm(id: &str, state: &str) -> VmSnapshot {
        VmSnapshot {
            id: id.to_owned(),
            organization_id: "O-1".to_owned(),
            owner_id: "TM-1".to_owned(),
            template: "developer".to_owned(),
            state: state.to_owned(),
            guest_ip: Some("172.16.0.9".to_owned()),
            vcpus: 2,
            mem_mib: 1024,
        }
    }

    /// Stub sandbox provisioning. `fails` makes `provision` return an error, so the start
    /// path's failure handling can be exercised without a runtime.
    #[derive(Default)]
    struct StubVmProvisioning {
        fails: bool,
        stopped: RwLock<Vec<String>>,
        provisioned_env: RwLock<Vec<(String, String)>>,
    }

    impl VmProvisioning for StubVmProvisioning {
        async fn provision(
            &self,
            _organization_id: &str,
            _agent_id: &str,
            request: ProvisionVmRequest,
        ) -> anyhow::Result<Option<VmSnapshot>> {
            *self
                .provisioned_env
                .write()
                .expect("test write lock poisoned") = request.env;
            if self.fails {
                return Err(anyhow!("no capacity"));
            }
            Ok(Some(stub_vm("VM-1", "running")))
        }

        async fn stop(&self, vm_id: &str) -> anyhow::Result<Option<VmSnapshot>> {
            self.stopped
                .write()
                .expect("test write lock poisoned")
                .push(vm_id.to_owned());
            Ok(Some(stub_vm(vm_id, "stopped")))
        }

        async fn get(&self, vm_id: &str) -> anyhow::Result<Option<VmSnapshot>> {
            Ok(Some(stub_vm(vm_id, "running")))
        }
    }

    type Svc = TeamApplicationService<
        TestTeamRepository,
        TestOrganizationRepository,
        TestBoardRepository,
        TestRepoRepository,
        StubVmProvisioning,
        StubTeamIdentity,
    >;

    const API_URL: &str = "https://wiab.example";

    fn service_with(vm: StubVmProvisioning) -> Svc {
        TeamApplicationService::new(
            TestTeamRepository::default(),
            TestOrganizationRepository::default(),
            TestBoardRepository::default(),
            TestRepoRepository::default(),
            vm,
            StubTeamIdentity::default(),
            Arc::new(TestTeamNumbering::default()),
            API_URL.to_owned(),
        )
    }

    fn service() -> Svc {
        service_with(StubVmProvisioning::default())
    }

    async fn seed_organization(service: &Svc, number: u64) -> String {
        let organization = Organization::new(
            OrganizationId::from_number(number),
            format!("Org {number}"),
            String::new(),
        )
        .unwrap();
        let id = organization.id().to_string();
        service
            .organization_repository
            .save(organization, Version::NEW)
            .await
            .unwrap();

        let board = Board::new(
            BoardId::from_number(1),
            ProjectId::from_number(1),
            "backlog".to_owned(),
            String::new(),
        )
        .unwrap();
        service
            .board_repository
            .save(board, Version::NEW)
            .await
            .unwrap();

        let repo = Repo::new(
            RepoId::from_number(7),
            ProjectId::from_number(1),
            "widgets".to_owned(),
            String::new(),
            Visibility::Private,
        )
        .unwrap();
        service
            .repo_repository
            .save(repo, Version::NEW)
            .await
            .unwrap();
        id
    }

    async fn create(service: &Svc, organization_id: &str, name: &str) -> TeamSnapshot {
        service
            .create_team(
                organization_id,
                CreateTeamRequest {
                    name: name.to_owned(),
                    description: String::new(),
                    board_id: "B-1".to_owned(),
                    repo_id: "R-7".to_owned(),
                    vm_type: "developer".to_owned(),
                },
            )
            .await
            .unwrap()
            .unwrap()
    }

    #[tokio::test]
    async fn creates_a_stopped_team_and_reads_it_back() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        let created = create(&service, &organization_id, "platform").await;
        assert_eq!(created.id, "TM-1");
        assert_eq!(created.state, "stopped");

        let read_back = service.team_snapshot("TM-1").await.unwrap().unwrap();
        assert_eq!(read_back, created);
    }

    #[tokio::test]
    async fn creating_under_an_unknown_organization_is_not_found() {
        let service = service();
        let outcome = service
            .create_team(
                "O-404",
                CreateTeamRequest {
                    name: "platform".to_owned(),
                    description: String::new(),
                    board_id: "B-1".to_owned(),
                    repo_id: "R-7".to_owned(),
                    vm_type: "developer".to_owned(),
                },
            )
            .await
            .unwrap();
        assert!(outcome.is_none());
    }

    #[tokio::test]
    async fn lists_only_the_organization_s_own_teams_in_id_order() {
        let service = service();
        let first = seed_organization(&service, 1).await;
        let second = seed_organization(&service, 2).await;
        create(&service, &first, "platform").await;
        create(&service, &second, "growth").await;
        create(&service, &first, "payments").await;

        let listed = service.list_teams(&first).await.unwrap().unwrap();
        let names: Vec<&str> = listed.iter().map(|team| team.name.as_str()).collect();
        assert_eq!(names, ["platform", "payments"]);
    }

    #[tokio::test]
    async fn listing_for_an_unknown_organization_is_not_found() {
        let service = service();
        assert!(service.list_teams("O-404").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn unknown_teams_are_not_found() {
        let service = service();
        assert!(service.team_snapshot("TM-404").await.unwrap().is_none());
        assert!(service.start_team("TM-404").await.unwrap().is_none());
        assert!(service.stop_team("TM-404").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn starting_provisions_a_sandbox_and_leaves_the_team_idle() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        create(&service, &organization_id, "platform").await;

        let started = service.start_team("TM-1").await.unwrap().unwrap();
        assert_eq!(started.state, "idle");
        assert_eq!(started.vm_id.as_deref(), Some("VM-1"));
    }

    #[tokio::test]
    async fn a_failed_provision_leaves_the_team_failed_not_stopped() {
        // Otherwise a start that never produced a container would look startable again while
        // the runtime may have half-booted one.
        let service = service_with(StubVmProvisioning {
            fails: true,
            ..StubVmProvisioning::default()
        });
        let organization_id = seed_organization(&service, 1).await;
        create(&service, &organization_id, "platform").await;

        assert!(service.start_team("TM-1").await.is_err());
        let team = service.team_snapshot("TM-1").await.unwrap().unwrap();
        assert_eq!(team.state, "failed");
        assert_eq!(team.vm_id, None);
    }

    #[tokio::test]
    async fn starting_a_running_team_is_rejected() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        create(&service, &organization_id, "platform").await;
        service.start_team("TM-1").await.unwrap();
        assert!(service.start_team("TM-1").await.is_err());
    }

    #[tokio::test]
    async fn pausing_keeps_the_container_and_resuming_needs_no_new_one() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        create(&service, &organization_id, "platform").await;
        service.start_team("TM-1").await.unwrap();

        let paused = service.pause_team("TM-1").await.unwrap().unwrap();
        assert_eq!(paused.state, "paused");
        assert_eq!(paused.vm_id.as_deref(), Some("VM-1"));
        assert!(
            service.vm.stopped.read().unwrap().is_empty(),
            "pausing must not stop the sandbox"
        );

        let resumed = service.resume_team("TM-1").await.unwrap().unwrap();
        assert_eq!(resumed.state, "idle");
        assert_eq!(resumed.vm_id.as_deref(), Some("VM-1"));
    }

    #[tokio::test]
    async fn resuming_a_team_that_is_not_paused_is_rejected() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        create(&service, &organization_id, "platform").await;
        service.start_team("TM-1").await.unwrap();
        assert!(service.resume_team("TM-1").await.is_err());
    }

    #[tokio::test]
    async fn stopping_releases_the_sandbox() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        create(&service, &organization_id, "platform").await;
        service.start_team("TM-1").await.unwrap();

        let stopped = service.stop_team("TM-1").await.unwrap().unwrap();
        assert_eq!(stopped.state, "stopped");
        assert_eq!(stopped.vm_id, None);
        assert_eq!(service.vm.stopped.read().unwrap().as_slice(), ["VM-1"]);
    }

    #[tokio::test]
    async fn stopping_a_team_that_never_started_asks_the_runtime_for_nothing() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        create(&service, &organization_id, "platform").await;

        let stopped = service.stop_team("TM-1").await.unwrap().unwrap();
        assert_eq!(stopped.state, "stopped");
        assert!(service.vm.stopped.read().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_stopped_team_can_be_started_again() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        create(&service, &organization_id, "platform").await;
        service.start_team("TM-1").await.unwrap();
        service.stop_team("TM-1").await.unwrap();

        let restarted = service.start_team("TM-1").await.unwrap().unwrap();
        assert_eq!(restarted.state, TeamState::Idle.to_string());
    }

    #[tokio::test]
    async fn a_started_team_is_told_which_board_to_poll_and_which_repo_to_clone() {
        // Without this the container starts, polls nothing, and looks idle rather than
        // misconfigured — the failure this whole change exists to prevent.
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        create(&service, &organization_id, "platform").await;
        service.start_team("TM-1").await.unwrap();

        let env = service.vm.provisioned_env.read().unwrap().clone();
        let value = |key: &str| {
            env.iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| panic!("{key} was not passed to the container"))
        };
        assert_eq!(value("WIAB_TEAM_API_URL"), "https://wiab.example");
        assert_eq!(value("WIAB_TEAM_TEAM_ID"), "TM-1");
        assert_eq!(value("WIAB_TEAM_BOARD_ID"), "B-1");
        // The clone URL is derived from the repo id, so the two cannot drift apart.
        assert_eq!(
            value("WIAB_TEAM_REPO_REMOTE"),
            "https://wiab.example/repos/R-7.git"
        );
        // The token is minted for this container, never stored.
        assert_eq!(value("WIAB_TEAM_API_TOKEN"), "tok-1");
    }

    #[test]
    fn a_trailing_slash_on_the_api_url_does_not_double_up() {
        let team = TeamSnapshot {
            id: "TM-1".to_owned(),
            organization_id: "O-1".to_owned(),
            name: "platform".to_owned(),
            description: String::new(),
            board_id: "B-2".to_owned(),
            repo_id: "R-3".to_owned(),
            user_id: "U-9".to_owned(),
            vm_template: "developer".to_owned(),
            state: "starting".to_owned(),
            vm_id: None,
        };
        let env = worker_env(&team, "https://wiab.example/", "tok");
        let value = |key: &str| {
            env.iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
                .unwrap()
        };
        assert_eq!(value("WIAB_TEAM_API_URL"), "https://wiab.example");
        assert_eq!(
            value("WIAB_TEAM_REPO_REMOTE"),
            "https://wiab.example/repos/R-3.git"
        );
    }

    #[tokio::test]
    async fn every_start_mints_a_fresh_token() {
        // A token lives only as long as the container it was issued to, so restarting a
        // team must not hand the new container the old credential.
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        create(&service, &organization_id, "platform").await;

        service.start_team("TM-1").await.unwrap();
        service.stop_team("TM-1").await.unwrap();
        service.start_team("TM-1").await.unwrap();

        assert_eq!(
            service.identity.tokens.read().unwrap().as_slice(),
            ["tok-1", "tok-2"]
        );
    }

    #[tokio::test]
    async fn a_created_team_is_given_its_own_identity() {
        let service = service();
        let organization_id = seed_organization(&service, 1).await;
        let created = create(&service, &organization_id, "platform").await;

        assert_eq!(created.user_id, "U-9");
        assert_eq!(
            service.identity.granted.read().unwrap().as_slice(),
            [("TM-1".to_owned(), "platform@O-1".to_owned())]
        );
    }
}
