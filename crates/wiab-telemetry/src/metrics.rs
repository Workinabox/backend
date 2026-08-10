//! Every metric instrument the backend records, created once against the
//! global meter provider.
//!
//! Naming: OTel semantic conventions where one exists, `wiab.*` otherwise.
//! Attribute sets are bounded enums by policy — user, VM, meeting, and repo
//! ids belong on spans, never on metric labels.

use std::future::Future;
use std::sync::OnceLock;
use std::time::Instant;

use opentelemetry::global;
use opentelemetry::metrics::{Counter, Histogram, UpDownCounter};
use tracing::Instrument;

/// Sub-second work: HTTP handling, database calls.
const SHORT_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];
/// Seconds-to-minutes work: LLM generation, VM lifecycle.
const LONG_BUCKETS: &[f64] = &[0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0];
/// Audio-chunk work: STT, opus encode, per-token decode.
const AUDIO_BUCKETS: &[f64] = &[0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0];

pub struct Metrics {
    pub http_request_duration: Histogram<f64>,
    pub auth_logins: Counter<u64>,
    pub authz_denials: Counter<u64>,
    pub db_operation_duration: Histogram<f64>,
    pub genai_operation_duration: Histogram<f64>,
    pub genai_token_usage: Histogram<u64>,
    pub genai_time_to_first_token: Histogram<f64>,
    pub genai_time_per_output_token: Histogram<f64>,
    pub llama_queue_duration: Histogram<f64>,
    pub stt_queue_depth: UpDownCounter<i64>,
    pub stt_transcription_duration: Histogram<f64>,
    pub stt_audio_duration: Counter<f64>,
    pub audio_decode_errors: Counter<u64>,
    pub audio_encode_duration: Histogram<f64>,
    pub audio_encode_errors: Counter<u64>,
    pub sfu_signal_sessions: UpDownCounter<i64>,
    pub sfu_peers: UpDownCounter<i64>,
    pub sfu_producers: UpDownCounter<i64>,
    pub sfu_consumers: UpDownCounter<i64>,
    pub sfu_transport_errors: Counter<u64>,
    pub vm_boot_duration: Histogram<f64>,
    pub vm_shutdown_duration: Histogram<f64>,
    pub vm_shutdown_forced: Counter<u64>,
    pub vms_active: UpDownCounter<i64>,
    pub vm_launch_errors: Counter<u64>,
    pub messaging_sent: Counter<u64>,
    pub git_ssh_operations: Counter<u64>,
}

/// The process-wide instruments. First call binds to whatever meter provider
/// is global at that point — [`crate::init`] installs it before anything can
/// record, and without one every instrument is a cheap no-op.
pub fn metrics() -> &'static Metrics {
    static METRICS: OnceLock<Metrics> = OnceLock::new();
    METRICS.get_or_init(Metrics::new)
}

/// Times one persistence call: a `db.client.operation.duration` sample plus a
/// debug-level span, so per-call spans exist only when `RUST_LOG` asks for
/// them. The dispatch enums wrap every delegation in this.
pub async fn timed_db<T, E>(
    repository: &'static str,
    operation: &'static str,
    backend: &'static str,
    future: impl Future<Output = Result<T, E>>,
) -> Result<T, E> {
    let span = tracing::debug_span!(
        "db",
        otel.name = %format_args!("db {operation}"),
        db.system.name = backend,
        db.operation.name = operation,
        wiab.repository = repository,
    );
    let started = Instant::now();
    let result = future.instrument(span).await;
    metrics().db_operation_duration.record(
        started.elapsed().as_secs_f64(),
        &[
            opentelemetry::KeyValue::new("db.system.name", backend),
            opentelemetry::KeyValue::new("db.operation.name", operation),
            opentelemetry::KeyValue::new("wiab.repository", repository),
        ],
    );
    result
}

impl Metrics {
    fn new() -> Self {
        let meter = global::meter("wiab");
        Self {
            http_request_duration: meter
                .f64_histogram("http.server.request.duration")
                .with_unit("s")
                .with_boundaries(SHORT_BUCKETS.to_vec())
                .build(),
            auth_logins: meter
                .u64_counter("wiab.auth.logins")
                .with_unit("{login}")
                .build(),
            authz_denials: meter
                .u64_counter("wiab.authz.denials")
                .with_unit("{denial}")
                .build(),
            db_operation_duration: meter
                .f64_histogram("db.client.operation.duration")
                .with_unit("s")
                .with_boundaries(SHORT_BUCKETS.to_vec())
                .build(),
            genai_operation_duration: meter
                .f64_histogram("gen_ai.client.operation.duration")
                .with_unit("s")
                .with_boundaries(LONG_BUCKETS.to_vec())
                .build(),
            genai_token_usage: meter
                .u64_histogram("gen_ai.client.token.usage")
                .with_unit("{token}")
                .build(),
            genai_time_to_first_token: meter
                .f64_histogram("gen_ai.server.time_to_first_token")
                .with_unit("s")
                .with_boundaries(AUDIO_BUCKETS.to_vec())
                .build(),
            genai_time_per_output_token: meter
                .f64_histogram("gen_ai.server.time_per_output_token")
                .with_unit("s")
                .with_boundaries(AUDIO_BUCKETS.to_vec())
                .build(),
            llama_queue_duration: meter
                .f64_histogram("wiab.llama.queue.duration")
                .with_unit("s")
                .with_boundaries(LONG_BUCKETS.to_vec())
                .build(),
            stt_queue_depth: meter
                .i64_up_down_counter("wiab.stt.queue.depth")
                .with_unit("{chunk}")
                .build(),
            stt_transcription_duration: meter
                .f64_histogram("wiab.stt.transcription.duration")
                .with_unit("s")
                .with_boundaries(AUDIO_BUCKETS.to_vec())
                .build(),
            stt_audio_duration: meter
                .f64_counter("wiab.stt.audio.duration")
                .with_unit("s")
                .build(),
            audio_decode_errors: meter
                .u64_counter("wiab.audio.decode.errors")
                .with_unit("{error}")
                .build(),
            audio_encode_duration: meter
                .f64_histogram("wiab.audio.encode.duration")
                .with_unit("s")
                .with_boundaries(AUDIO_BUCKETS.to_vec())
                .build(),
            audio_encode_errors: meter
                .u64_counter("wiab.audio.encode.errors")
                .with_unit("{error}")
                .build(),
            sfu_signal_sessions: meter
                .i64_up_down_counter("wiab.sfu.signal.sessions.active")
                .with_unit("{session}")
                .build(),
            sfu_peers: meter
                .i64_up_down_counter("wiab.sfu.peers.active")
                .with_unit("{peer}")
                .build(),
            sfu_producers: meter
                .i64_up_down_counter("wiab.sfu.producers.active")
                .with_unit("{producer}")
                .build(),
            sfu_consumers: meter
                .i64_up_down_counter("wiab.sfu.consumers.active")
                .with_unit("{consumer}")
                .build(),
            sfu_transport_errors: meter
                .u64_counter("wiab.sfu.transport.errors")
                .with_unit("{error}")
                .build(),
            vm_boot_duration: meter
                .f64_histogram("wiab.vm.boot.duration")
                .with_unit("s")
                .with_boundaries(LONG_BUCKETS.to_vec())
                .build(),
            vm_shutdown_duration: meter
                .f64_histogram("wiab.vm.shutdown.duration")
                .with_unit("s")
                .with_boundaries(LONG_BUCKETS.to_vec())
                .build(),
            vm_shutdown_forced: meter
                .u64_counter("wiab.vm.shutdown.forced")
                .with_unit("{shutdown}")
                .build(),
            vms_active: meter
                .i64_up_down_counter("wiab.vms.active")
                .with_unit("{vm}")
                .build(),
            vm_launch_errors: meter
                .u64_counter("wiab.vm.launch.errors")
                .with_unit("{error}")
                .build(),
            messaging_sent: meter
                .u64_counter("messaging.client.sent.messages")
                .with_unit("{message}")
                .build(),
            git_ssh_operations: meter
                .u64_counter("wiab.git.ssh.operations")
                .with_unit("{operation}")
                .build(),
        }
    }
}
