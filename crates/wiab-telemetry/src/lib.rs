//! Telemetry for the wiab backend: subscriber setup, JSON logs carrying trace
//! ids, optional OTLP export of traces/metrics/logs, metric instruments, and
//! W3C trace-context propagation helpers.
//!
//! With no configuration at all, the backend emits JSON log lines to stdout
//! (journald's capture surface), spans are created so log lines share a
//! `trace_id`, and nothing is exported anywhere. Setting
//! `OTEL_EXPORTER_OTLP_ENDPOINT` turns on OTLP export of all three signals;
//! `WIAB_OTEL_CONSOLE=1` dumps spans and metrics to stdout for local debugging.
//!
//! # The audit stream
//!
//! Audit events are ordinary tracing events with the literal target
//! [`AUDIT_TARGET`]:
//!
//! ```ignore
//! tracing::info!(target: "audit", event = "auth.login", outcome = "failure",
//!                reason = "invalid_credentials");
//! ```
//!
//! The convention (kept as a literal so `authbox-app` needs no dependency on
//! this crate):
//!
//! - `event` — dot-namespaced name (`auth.login`, `user.token.issued`,
//!   `access.role.granted`, `authz.denied`, ...).
//! - `outcome` — `success` | `failure` | `denied`.
//! - `actor` — the acting principal's opaque id. Never an email address.
//! - `reason` — a short enum-ish string on failures.
//! - Resource ids as applicable (`user_id`, `org_id`, `role`, `token_id`, ...).
//! - Never: emails, passwords, tokens, cookie secrets, SSH keys (fingerprints
//!   are fine), prompts, or transcripts.
//!
//! Audit lines always reach stdout with `"stream":"audit"`, unfiltered by
//! `RUST_LOG` and never sampled; they are also included in OTLP log export
//! when an endpoint is configured.

pub mod config;
mod init;
mod json_format;
mod metrics;
mod propagation;

pub use config::TelemetryConfig;
pub use init::{TelemetryGuard, init};
pub use metrics::{Metrics, metrics, timed_db};
pub use propagation::{current_traceparent, extract_context};

/// The tracing `target:` literal that routes an event to the audit stream.
pub const AUDIT_TARGET: &str = "audit";
