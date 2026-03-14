#[derive(Debug, Clone)]
pub struct TranscriptIdentity {
    pub meeting_id: String,
    pub peer_id: String,
    pub track_id: String,
}

#[derive(Debug)]
pub struct TranscriptJob {
    pub identity: TranscriptIdentity,
    pub chunk_index: u64,
    pub pcm_16k_mono: Vec<f32>,
}

#[derive(Debug, Clone)]
pub struct FinalizedTranscript {
    pub identity: TranscriptIdentity,
    pub chunk_index: u64,
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_identity_construction_and_access() {
        let id = TranscriptIdentity {
            meeting_id: "meeting-1".to_owned(),
            peer_id: "peer-1".to_owned(),
            track_id: "track-1".to_owned(),
        };
        assert_eq!(id.meeting_id, "meeting-1");
        assert_eq!(id.peer_id, "peer-1");
        assert_eq!(id.track_id, "track-1");
    }

    #[test]
    fn transcript_identity_clone() {
        let id = TranscriptIdentity {
            meeting_id: "m".to_owned(),
            peer_id: "p".to_owned(),
            track_id: "t".to_owned(),
        };
        let cloned = id.clone();
        assert_eq!(cloned.meeting_id, id.meeting_id);
        assert_eq!(cloned.peer_id, id.peer_id);
        assert_eq!(cloned.track_id, id.track_id);
    }

    #[test]
    fn transcript_identity_debug() {
        let id = TranscriptIdentity {
            meeting_id: "m".to_owned(),
            peer_id: "p".to_owned(),
            track_id: "t".to_owned(),
        };
        let s = format!("{id:?}");
        assert!(s.contains("meeting_id"));
    }

    #[test]
    fn transcript_job_construction_and_debug() {
        let job = TranscriptJob {
            identity: TranscriptIdentity {
                meeting_id: "m".to_owned(),
                peer_id: "p".to_owned(),
                track_id: "t".to_owned(),
            },
            chunk_index: 7,
            pcm_16k_mono: vec![0.1, 0.2],
        };
        assert_eq!(job.chunk_index, 7);
        assert_eq!(job.pcm_16k_mono.len(), 2);
        let s = format!("{job:?}");
        assert!(s.contains("chunk_index"));
    }

    #[test]
    fn finalized_transcript_clone() {
        let transcript = FinalizedTranscript {
            identity: TranscriptIdentity {
                meeting_id: "m".to_owned(),
                peer_id: "p".to_owned(),
                track_id: "t".to_owned(),
            },
            chunk_index: 9,
            text: "hello".to_owned(),
        };
        let cloned = transcript.clone();
        assert_eq!(cloned.identity.meeting_id, "m");
        assert_eq!(cloned.chunk_index, 9);
        assert_eq!(cloned.text, "hello");
    }
}
