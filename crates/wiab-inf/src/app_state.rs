use std::sync::Arc;

use wiab_app::MeetingApplicationService;

use crate::{InMemoryMeetingRepository, Sfu};

#[derive(Clone)]
pub struct AppState {
    pub meeting_service: Arc<MeetingApplicationService<InMemoryMeetingRepository>>,
    pub sfu: Arc<Sfu>,
}
