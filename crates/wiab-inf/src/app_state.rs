use std::sync::Arc;

use wiab_app::{MeetingApplicationService, WorkApplicationService};

use crate::{InMemoryMeetingRepository, InMemoryWorkRepository, Sfu};

#[derive(Clone)]
pub struct AppState {
    pub meeting_service: Arc<MeetingApplicationService<InMemoryMeetingRepository>>,
    pub work_service: Arc<WorkApplicationService<InMemoryWorkRepository>>,
    pub sfu: Arc<Sfu>,
}
