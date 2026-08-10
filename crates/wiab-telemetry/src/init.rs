//! Subscriber and provider construction.
//!
//! Layer layout, from the registry outward:
//!
//! - OTel span layer (filtered by `RUST_LOG`) — spans always get valid ids so
//!   log lines correlate; whether they are *exported* depends on the endpoint.
//! - OTLP log layer (endpoint set only) — `RUST_LOG` plus the audit stream,
//!   with the exporter's own HTTP stack hard-muted so its logs cannot feed
//!   back into itself.
//! - Debug log layer — JSON to stdout, filtered by `RUST_LOG`, audit excluded.
//! - Audit log layer — JSON to stdout, target `audit` only, no `RUST_LOG`
//!   filter and no sampling: `RUST_LOG=error` cannot suppress the audit trail.

use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, filter, fmt};

use crate::AUDIT_TARGET;
use crate::config::TelemetryConfig;
use crate::json_format::WiabJson;

/// Holds the providers so batches can be flushed at shutdown. Call
/// [`TelemetryGuard::shutdown`] before the process exits; `Drop` is only the
/// best-effort fallback.
pub struct TelemetryGuard {
    tracer_provider: SdkTracerProvider,
    meter_provider: SdkMeterProvider,
    logger_provider: Option<SdkLoggerProvider>,
    shut_down: bool,
}

impl TelemetryGuard {
    pub fn shutdown(mut self) {
        self.flush();
    }

    fn flush(&mut self) {
        if self.shut_down {
            return;
        }
        self.shut_down = true;
        // Failures go to stderr: tracing itself may be mid-teardown here.
        if let Err(error) = self.tracer_provider.shutdown() {
            eprintln!("telemetry: trace shutdown failed: {error}");
        }
        if let Err(error) = self.meter_provider.shutdown() {
            eprintln!("telemetry: metrics shutdown failed: {error}");
        }
        if let Some(logger_provider) = &self.logger_provider
            && let Err(error) = logger_provider.shutdown()
        {
            eprintln!("telemetry: log shutdown failed: {error}");
        }
    }
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        self.flush();
    }
}

/// Install the global subscriber, propagator, and meter provider. Call once,
/// before configuration loading, so config errors are logged rather than lost.
pub fn init(config: &TelemetryConfig, service_version: &str) -> anyhow::Result<TelemetryGuard> {
    global::set_text_map_propagator(TraceContextPropagator::new());

    let resource = Resource::builder()
        .with_service_name(config.service_name.clone())
        .with_attribute(opentelemetry::KeyValue::new(
            "service.version",
            service_version.to_owned(),
        ))
        .build();

    let endpoint = config
        .otlp_endpoint
        .as_deref()
        .map(|endpoint| endpoint.trim_end_matches('/').to_owned());

    // Traces. Always build the provider: with no processor attached, spans
    // still carry valid sampled ids (so logs correlate) and export is a no-op.
    let mut tracer_builder = SdkTracerProvider::builder().with_resource(resource.clone());
    if let Some(endpoint) = &endpoint {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(format!("{endpoint}/v1/traces"))
            .build()?;
        tracer_builder = tracer_builder.with_batch_exporter(exporter);
    }
    if config.console {
        tracer_builder =
            tracer_builder.with_batch_exporter(opentelemetry_stdout::SpanExporter::default());
    }
    let tracer_provider = tracer_builder.build();
    let tracer = tracer_provider.tracer("wiab");

    // Metrics: no reader unless something exports, which keeps every
    // instrument a no-op by default.
    let mut meter_builder = SdkMeterProvider::builder().with_resource(resource.clone());
    if let Some(endpoint) = &endpoint {
        let exporter = opentelemetry_otlp::MetricExporter::builder()
            .with_http()
            .with_endpoint(format!("{endpoint}/v1/metrics"))
            .build()?;
        meter_builder = meter_builder.with_periodic_exporter(exporter);
    }
    if config.console {
        meter_builder =
            meter_builder.with_periodic_exporter(opentelemetry_stdout::MetricExporter::default());
    }
    let meter_provider = meter_builder.build();
    global::set_meter_provider(meter_provider.clone());

    // Logs over OTLP, endpoint only. The filter adds the audit stream on top
    // of RUST_LOG and hard-mutes the exporter's own HTTP stack — otherwise the
    // export client's logs would re-enter the pipeline they feed.
    let (logger_provider, otlp_log_layer) = match &endpoint {
        Some(endpoint) => {
            let exporter = opentelemetry_otlp::LogExporter::builder()
                .with_http()
                .with_endpoint(format!("{endpoint}/v1/logs"))
                .build()?;
            let logger_provider = SdkLoggerProvider::builder()
                .with_resource(resource)
                .with_batch_exporter(exporter)
                .build();
            let layer = opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(
                &logger_provider,
            )
            .with_filter(EnvFilter::new(format!(
                "{},{AUDIT_TARGET}=trace,opentelemetry=off,hyper=off,reqwest=off,h2=off",
                config.rust_log
            )));
            (Some(logger_provider), Some(layer))
        }
        None => (None, None),
    };

    // Filled after installation; the formatter reads OTel ids through it (see
    // the note on `WiabJson`).
    let dispatch_slot: std::sync::Arc<std::sync::OnceLock<tracing::Dispatch>> =
        std::sync::Arc::new(std::sync::OnceLock::new());

    let debug_log_layer = fmt::layer()
        .event_format(WiabJson::new(dispatch_slot.clone()))
        .with_filter(EnvFilter::new(&config.rust_log))
        .with_filter(filter::filter_fn(|metadata| {
            metadata.target() != AUDIT_TARGET
        }));

    let audit_log_layer = fmt::layer()
        .event_format(WiabJson::new(dispatch_slot.clone()))
        .with_filter(filter::filter_fn(|metadata| {
            metadata.target() == AUDIT_TARGET
        }));

    tracing_subscriber::registry()
        .with(
            tracing_opentelemetry::layer()
                .with_tracer(tracer)
                .with_filter(EnvFilter::new(&config.rust_log)),
        )
        .with(otlp_log_layer)
        .with(debug_log_layer)
        .with(audit_log_layer)
        .init();
    let _ = dispatch_slot.set(tracing::dispatcher::get_default(|dispatch| {
        dispatch.clone()
    }));

    Ok(TelemetryGuard {
        tracer_provider,
        meter_provider,
        logger_provider,
        shut_down: false,
    })
}
