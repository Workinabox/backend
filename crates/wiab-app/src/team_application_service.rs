use std::sync::Arc;

use anyhow::anyhow;
use wiab_core::organization::{OrganizationId, OrganizationRepository};
use wiab_core::repository::{SaveError, Version};
use wiab_core::team::{Team, TeamId, TeamNumbering, TeamRepository, TeamSnapshot};
use wiab_core::vm::{VmId, VmTemplate};

use crate::team_requests::CreateTeamRequest;
use crate::vm_provisioning::VmProvisioning;
use crate::vm_requests::ProvisionVmRequest;

/// Orchestrates use cases over the `Team` aggregate — creation, and the start/pause/resume/stop
/// lifecycle. Starting provisions a sandbox through the [`VmProvisioning`] port (the same port
/// agent activation uses) and records the VM on the team; stopping releases it. Pausing does
/// not: a paused team keeps its container so resuming needs no re-provisioning.
///
/// Mutations use optimistic concurrency: load with version, apply, retry on conflict. Holds the
/// organization repository to verify the parent org exists.
pub struct TeamApplicationService<T: TeamRepository, O: OrganizationRepository, V: VmProvisioning> {
    team_repository: T,
    organization_repository: O,
    vm: V,
    numbering: Arc<dyn TeamNumbering>,
}

impl<T: TeamRepository, O: OrganizationRepository, V: VmProvisioning>
    TeamApplicationService<T, O, V>
{
    pub fn new(
        team_repository: T,
        organization_repository: O,
        vm: V,
        numbering: Arc<dyn TeamNumbering>,
    ) -> Self {
        Self {
            team_repository,
            organization_repository,
            vm,
            numbering,
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

    /// Returns `Ok(None)` when no organization with the given id exists.
    pub async fn create_team(
        &self,
        organization_id: &str,
        request: CreateTeamRequest,
    ) -> anyhow::Result<Option<TeamSnapshot>> {
        let id: OrganizationId = organization_id.parse()?;
        if self.organization_repository.get(&id).await?.is_none() {
            return Ok(None);
        }
        let team = Team::new(
            self.numbering.next(),
            id,
            request.name,
            request.description,
            VmTemplate::new(request.vm_type)?,
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
        let request = ProvisionVmRequest {
            template: team.vm_template.clone(),
            vcpus: None,
            mem_mib: None,
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

    use wiab_core::organization::Organization;
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
    }

    impl VmProvisioning for StubVmProvisioning {
        async fn provision(
            &self,
            _organization_id: &str,
            _agent_id: &str,
            _request: ProvisionVmRequest,
        ) -> anyhow::Result<Option<VmSnapshot>> {
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

    type Svc =
        TeamApplicationService<TestTeamRepository, TestOrganizationRepository, StubVmProvisioning>;

    fn service_with(vm: StubVmProvisioning) -> Svc {
        TeamApplicationService::new(
            TestTeamRepository::default(),
            TestOrganizationRepository::default(),
            vm,
            Arc::new(TestTeamNumbering::default()),
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
        id
    }

    async fn create(service: &Svc, organization_id: &str, name: &str) -> TeamSnapshot {
        service
            .create_team(
                organization_id,
                CreateTeamRequest {
                    name: name.to_owned(),
                    description: String::new(),
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
}
