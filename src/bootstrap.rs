use std::sync::Arc;

use anyhow::Context;
use tokio::sync::mpsc;
use tracing::info;
use wiab_app::MeetingApplicationService;
use wiab_core::{agent::Clock, transcript::FinalizedTranscript};
use wiab_inf::{
    AppState, DefaultSpeechSynthesizer, HeuristicMeetingIntelligence, InMemoryMeetingRepository,
    Sfu, SystemClock,
};

pub async fn build_app_state() -> anyhow::Result<AppState> {
    let seed_clock = SystemClock;
    let meeting_repository = InMemoryMeetingRepository::with_seed_data(|| seed_clock.now_rfc3339());
    let meeting_service = Arc::new(MeetingApplicationService::new(
        meeting_repository.clone(),
        Arc::new(HeuristicMeetingIntelligence),
        Arc::new(DefaultSpeechSynthesizer::from_env()),
        Arc::new(SystemClock),
    ));

    log_loaded_meetings(meeting_service.as_ref());

    let (transcript_tx, transcript_rx) = mpsc::unbounded_channel::<FinalizedTranscript>();
    let sfu = Arc::new(
        Sfu::new(meeting_service.clone(), transcript_tx)
            .await
            .context("failed to initialize SFU")?,
    );
    spawn_transcript_runtime(sfu.clone(), transcript_rx);

    Ok(AppState {
        meeting_service,
        sfu,
    })
}

fn log_loaded_meetings(meeting_service: &MeetingApplicationService<InMemoryMeetingRepository>) {
    let meetings = meeting_service.list_meetings();
    info!("loaded {} meetings from startup data", meetings.len());
    for meeting in &meetings {
        info!("meeting '{}' state {:?}", meeting.title, meeting.state);
    }
}

fn spawn_transcript_runtime(
    sfu: Arc<Sfu>,
    mut transcript_rx: mpsc::UnboundedReceiver<FinalizedTranscript>,
) {
    tokio::spawn(async move {
        while let Some(transcript) = transcript_rx.recv().await {
            sfu.handle_finalized_transcript(transcript).await;
        }
    });
}
