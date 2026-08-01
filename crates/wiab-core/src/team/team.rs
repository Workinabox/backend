use crate::organization::OrganizationId;
use crate::team::{TeamError, TeamId, TeamSnapshot, TeamState};
use crate::vm::{VmId, VmTemplate};

/// A long-lived worker that pulls issues from the board and runs them one at a time.
///
/// Unlike an `Agent`, which is activated for a single piece of work, a team is started once
/// and keeps running between issues — so its lifecycle is a state machine rather than a
/// boolean, and pausing it does not release its container.
///
/// `Team` is an aggregate root; it references its organization and its VM by identity only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Team {
    id: TeamId,
    organization_id: OrganizationId,
    name: String,
    description: String,
    /// Which sandbox image to launch. Required, unlike `Agent`'s optional template: a team
    /// with no template could never start, so there is no point being able to create one.
    vm_template: VmTemplate,
    state: TeamState,
    vm_id: Option<VmId>,
}

impl Team {
    pub fn new(
        id: TeamId,
        organization_id: OrganizationId,
        name: String,
        description: String,
        vm_template: VmTemplate,
    ) -> Result<Self, TeamError> {
        if name.trim().is_empty() {
            return Err(TeamError::EmptyName);
        }
        Ok(Self {
            id,
            organization_id,
            name,
            description,
            vm_template,
            state: TeamState::Stopped,
            vm_id: None,
        })
    }

    /// Rebuild from persisted state, including states `new` cannot produce.
    #[allow(clippy::too_many_arguments)]
    pub fn from_persistence(
        id: TeamId,
        organization_id: OrganizationId,
        name: String,
        description: String,
        vm_template: VmTemplate,
        state: TeamState,
        vm_id: Option<VmId>,
    ) -> Self {
        Self {
            id,
            organization_id,
            name,
            description,
            vm_template,
            state,
            vm_id,
        }
    }

    pub fn id(&self) -> TeamId {
        self.id
    }

    pub fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn vm_template(&self) -> &VmTemplate {
        &self.vm_template
    }

    pub fn state(&self) -> TeamState {
        self.state
    }

    pub fn vm_id(&self) -> Option<VmId> {
        self.vm_id
    }

    /// Begin starting the team. Only legal from `Stopped`, so a second start cannot
    /// provision a second container for the same team.
    pub fn start(&mut self) -> Result<(), TeamError> {
        if self.state != TeamState::Stopped {
            return Err(TeamError::NotStopped(self.state));
        }
        self.state = TeamState::Starting;
        Ok(())
    }

    /// The container is up and the team is waiting for work. Only legal from `Starting`.
    pub fn mark_idle(&mut self, vm_id: VmId) -> Result<(), TeamError> {
        if self.state != TeamState::Starting {
            return Err(TeamError::NotStarting(self.state));
        }
        self.state = TeamState::Idle;
        self.vm_id = Some(vm_id);
        Ok(())
    }

    /// The team has taken an issue. Legal from `Idle` only — a paused team must not pick
    /// work up, which is the entire point of pausing it.
    pub fn mark_working(&mut self) -> Result<(), TeamError> {
        if self.state != TeamState::Idle {
            return Err(TeamError::NotRunning(self.state));
        }
        self.state = TeamState::Working;
        Ok(())
    }

    /// The team finished an issue and is free again.
    ///
    /// A pause requested mid-issue lands here: the team keeps its container, so it settles
    /// into `Paused` rather than `Idle` and takes nothing new.
    pub fn finish_work(&mut self, pause_requested: bool) -> Result<(), TeamError> {
        if self.state != TeamState::Working {
            return Err(TeamError::NotRunning(self.state));
        }
        self.state = if pause_requested {
            TeamState::Paused
        } else {
            TeamState::Idle
        };
        Ok(())
    }

    /// Stop taking new issues. An idle team pauses at once; a working team is recorded as
    /// paused only when its current issue finishes, so this is legal from `Idle` alone.
    pub fn pause(&mut self) -> Result<(), TeamError> {
        if self.state != TeamState::Idle {
            return Err(TeamError::NotRunning(self.state));
        }
        self.state = TeamState::Paused;
        Ok(())
    }

    /// Take work again. The container never stopped, so there is nothing to provision.
    pub fn resume(&mut self) -> Result<(), TeamError> {
        if self.state != TeamState::Paused {
            return Err(TeamError::NotPaused(self.state));
        }
        self.state = TeamState::Idle;
        Ok(())
    }

    /// Tear the team down. Legal from any provisioned state — stopping a working team is a
    /// deliberate operator choice, not an error — and idempotent from `Stopped`.
    pub fn stop(&mut self) {
        self.state = TeamState::Stopped;
        self.vm_id = None;
    }

    /// The team could not be started. Terminal until started again; clears any VM.
    pub fn mark_failed(&mut self) {
        self.state = TeamState::Failed;
        self.vm_id = None;
    }

    pub fn snapshot(&self) -> TeamSnapshot {
        TeamSnapshot {
            id: self.id.to_string(),
            organization_id: self.organization_id.to_string(),
            name: self.name.clone(),
            description: self.description.clone(),
            vm_template: self.vm_template.to_string(),
            state: self.state.to_string(),
            vm_id: self.vm_id.map(|id| id.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template() -> VmTemplate {
        VmTemplate::new("developer".to_owned()).unwrap()
    }

    fn team() -> Team {
        Team::new(
            TeamId::from_number(1),
            OrganizationId::from_number(2),
            "platform".to_owned(),
            "the platform team".to_owned(),
            template(),
        )
        .unwrap()
    }

    /// Drive a team to `Idle`, the state most transitions start from.
    fn running_team() -> Team {
        let mut team = team();
        team.start().unwrap();
        team.mark_idle(VmId::from_number(5)).unwrap();
        team
    }

    #[test]
    fn a_new_team_is_stopped_with_no_container() {
        let team = team();
        assert_eq!(team.state(), TeamState::Stopped);
        assert_eq!(team.vm_id(), None);
    }

    #[test]
    fn exposes_getters() {
        let team = team();
        assert_eq!(team.id(), TeamId::from_number(1));
        assert_eq!(team.organization_id(), OrganizationId::from_number(2));
        assert_eq!(team.name(), "platform");
        assert_eq!(team.description(), "the platform team");
        assert_eq!(team.vm_template(), &template());
    }

    #[test]
    fn rejects_a_blank_name() {
        let error = Team::new(
            TeamId::from_number(1),
            OrganizationId::from_number(2),
            "   ".to_owned(),
            String::new(),
            template(),
        )
        .unwrap_err();
        assert_eq!(error, TeamError::EmptyName);
    }

    #[test]
    fn starting_twice_is_rejected() {
        let mut team = team();
        team.start().unwrap();
        // Otherwise a second start would provision a second container for one team.
        assert_eq!(
            team.start().unwrap_err(),
            TeamError::NotStopped(TeamState::Starting)
        );
    }

    #[test]
    fn marking_idle_records_the_container() {
        let team = running_team();
        assert_eq!(team.state(), TeamState::Idle);
        assert_eq!(team.vm_id(), Some(VmId::from_number(5)));
    }

    #[test]
    fn marking_idle_before_starting_is_rejected() {
        let mut team = team();
        assert_eq!(
            team.mark_idle(VmId::from_number(5)).unwrap_err(),
            TeamError::NotStarting(TeamState::Stopped)
        );
    }

    #[test]
    fn an_idle_team_takes_work_and_returns_to_idle() {
        let mut team = running_team();
        team.mark_working().unwrap();
        assert_eq!(team.state(), TeamState::Working);
        team.finish_work(false).unwrap();
        assert_eq!(team.state(), TeamState::Idle);
    }

    #[test]
    fn a_paused_team_will_not_take_work() {
        let mut team = running_team();
        team.pause().unwrap();
        assert_eq!(
            team.mark_working().unwrap_err(),
            TeamError::NotRunning(TeamState::Paused)
        );
    }

    #[test]
    fn a_pause_requested_mid_issue_lands_after_the_issue() {
        let mut team = running_team();
        team.mark_working().unwrap();
        team.finish_work(true).unwrap();
        assert_eq!(team.state(), TeamState::Paused);
        // The container is kept, so resuming needs no re-provisioning.
        assert_eq!(team.vm_id(), Some(VmId::from_number(5)));
    }

    #[test]
    fn pausing_a_working_team_directly_is_rejected() {
        // The application layer records the request; the team settles on finish_work.
        let mut team = running_team();
        team.mark_working().unwrap();
        assert_eq!(
            team.pause().unwrap_err(),
            TeamError::NotRunning(TeamState::Working)
        );
    }

    #[test]
    fn resume_returns_a_paused_team_to_idle_keeping_its_container() {
        let mut team = running_team();
        team.pause().unwrap();
        team.resume().unwrap();
        assert_eq!(team.state(), TeamState::Idle);
        assert_eq!(team.vm_id(), Some(VmId::from_number(5)));
    }

    #[test]
    fn resuming_a_team_that_is_not_paused_is_rejected() {
        let mut team = running_team();
        assert_eq!(
            team.resume().unwrap_err(),
            TeamError::NotPaused(TeamState::Idle)
        );
    }

    #[test]
    fn stopping_releases_the_container_from_any_state() {
        for drive in [
            (|t: &mut Team| t.pause().unwrap()) as fn(&mut Team),
            |t: &mut Team| t.mark_working().unwrap(),
            |_: &mut Team| {},
        ] {
            let mut team = running_team();
            drive(&mut team);
            team.stop();
            assert_eq!(team.state(), TeamState::Stopped);
            assert_eq!(team.vm_id(), None);
        }
    }

    #[test]
    fn stopping_is_idempotent() {
        let mut team = team();
        team.stop();
        team.stop();
        assert_eq!(team.state(), TeamState::Stopped);
    }

    #[test]
    fn a_failed_team_keeps_no_container() {
        let mut team = team();
        team.start().unwrap();
        team.mark_failed();
        assert_eq!(team.state(), TeamState::Failed);
        assert_eq!(team.vm_id(), None);
    }

    #[test]
    fn a_stopped_team_can_be_started_again() {
        // Stopped is where a team begins and ends; it is not terminal.
        let mut team = running_team();
        team.stop();
        team.start().unwrap();
        assert_eq!(team.state(), TeamState::Starting);
    }

    #[test]
    fn snapshot_mirrors_the_team() {
        let team = running_team();
        let snapshot = team.snapshot();
        assert_eq!(snapshot.id, "TM-1");
        assert_eq!(snapshot.organization_id, "O-2");
        assert_eq!(snapshot.name, "platform");
        assert_eq!(snapshot.description, "the platform team");
        assert_eq!(snapshot.vm_template, "developer");
        assert_eq!(snapshot.state, "idle");
        assert_eq!(snapshot.vm_id.as_deref(), Some("VM-5"));
    }

    #[test]
    fn from_persistence_round_trips_a_paused_team() {
        let team = Team::from_persistence(
            TeamId::from_number(1),
            OrganizationId::from_number(2),
            "platform".to_owned(),
            String::new(),
            template(),
            TeamState::Paused,
            Some(VmId::from_number(5)),
        );
        assert_eq!(team.state(), TeamState::Paused);
        assert_eq!(team.vm_id(), Some(VmId::from_number(5)));
    }
}
