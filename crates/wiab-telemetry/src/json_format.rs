//! One JSON line per event, with the OTel trace context inlined.
//!
//! The stock JSON formatter cannot include `trace_id`/`span_id` — they live in
//! the `tracing-opentelemetry` layer's span extension, which only a custom
//! [`FormatEvent`] can reach. Output shape:
//!
//! ```json
//! {"timestamp":"…","level":"INFO","target":"wiab_inf::sfu","message":"…",
//!  "span":"vm.launch","trace_id":"…","span_id":"…", …event fields…}
//! ```
//!
//! Audit events (target `audit`) additionally carry `"stream":"audit"`.

use std::sync::{Arc, OnceLock};

use opentelemetry::trace::TraceContextExt;
use serde_json::{Map, Value};
use tracing::field::{Field, Visit};
use tracing::{Dispatch, Event, Subscriber};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::{FormatTime, SystemTime};
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;

use crate::AUDIT_TARGET;

/// Formats events as JSON lines, reading OTel ids through the [`Dispatch`]
/// placed in the shared slot after the subscriber is installed.
///
/// The slot exists because `dispatcher::get_default` cannot be used here: it
/// returns the no-op dispatcher while an event is being dispatched (tracing's
/// re-entrancy guard), which is exactly when `format_event` runs.
pub struct WiabJson {
    dispatch: Arc<OnceLock<Dispatch>>,
}

impl WiabJson {
    pub fn new(dispatch: Arc<OnceLock<Dispatch>>) -> Self {
        Self { dispatch }
    }
}

impl<S, N> FormatEvent<S, N> for WiabJson
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> std::fmt::Result {
        let mut fields = Map::new();
        event.record(&mut JsonVisitor(&mut fields));
        let message = match fields.remove("message") {
            Some(Value::String(message)) => message,
            Some(other) => other.to_string(),
            None => String::new(),
        };

        let mut line = Map::new();
        line.insert("timestamp".to_owned(), Value::String(timestamp()));
        line.insert(
            "level".to_owned(),
            Value::String(event.metadata().level().to_string()),
        );
        line.insert(
            "target".to_owned(),
            Value::String(event.metadata().target().to_owned()),
        );
        line.insert("message".to_owned(), Value::String(message));
        if event.metadata().target() == AUDIT_TARGET {
            line.insert("stream".to_owned(), Value::String("audit".to_owned()));
        }

        if let Some(leaf) = ctx.event_scope().and_then(|mut scope| scope.next()) {
            line.insert("span".to_owned(), Value::String(leaf.name().to_owned()));
            // The OTel layer owns the ids; `get_otel_context` is its public
            // door to them (and inert when the layer filtered the span out).
            let context = self
                .dispatch
                .get()
                .and_then(|dispatch| tracing_opentelemetry::get_otel_context(&leaf.id(), dispatch));
            if let Some(context) = context {
                let span = context.span();
                let span_context = span.span_context();
                if span_context.is_valid() {
                    line.insert(
                        "trace_id".to_owned(),
                        Value::String(span_context.trace_id().to_string()),
                    );
                    line.insert(
                        "span_id".to_owned(),
                        Value::String(span_context.span_id().to_string()),
                    );
                }
            }
        }

        for (key, value) in fields {
            line.entry(key).or_insert(value);
        }

        writeln!(writer, "{}", Value::Object(line))
    }
}

fn timestamp() -> String {
    let mut out = String::with_capacity(32);
    let mut writer = Writer::new(&mut out);
    if SystemTime.format_time(&mut writer).is_err() {
        out.clear();
    }
    out
}

struct JsonVisitor<'a>(&'a mut Map<String, Value>);

impl Visit for JsonVisitor<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0
            .insert(field.name().to_owned(), Value::String(value.to_owned()));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().to_owned(), Value::from(value));
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.0
            .insert(field.name().to_owned(), Value::String(value.to_string()));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().to_owned(), Value::String(format!("{value:?}")));
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::{Arc, Mutex};

    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use tracing::subscriber::with_default;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::{EnvFilter, Layer, filter, fmt};

    use super::*;

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Capture {
        fn lines(&self) -> Vec<Value> {
            let bytes = self.0.lock().unwrap();
            String::from_utf8_lossy(&bytes)
                .lines()
                .map(|line| serde_json::from_str(line).expect("every line is JSON"))
                .collect()
        }
    }

    impl io::Write for Capture {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> fmt::MakeWriter<'a> for Capture {
        type Writer = Capture;

        fn make_writer(&'a self) -> Capture {
            self.clone()
        }
    }

    #[test]
    fn a_log_line_inside_a_span_carries_the_trace_context() {
        let capture = Capture::default();
        let provider = SdkTracerProvider::builder().build();
        let slot = Arc::new(OnceLock::new());
        let subscriber = tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")))
            .with(
                fmt::layer()
                    .event_format(WiabJson::new(slot.clone()))
                    .with_writer(capture.clone()),
            );
        let dispatch = Dispatch::new(subscriber);
        slot.set(dispatch.clone()).ok();
        tracing::dispatcher::with_default(&dispatch, || {
            let span = tracing::info_span!("vm.launch");
            let _guard = span.enter();
            tracing::info!(vm_id = "VM-1", "launching");
        });

        let lines = capture.lines();
        let line = lines.last().expect("one line");
        assert_eq!(line["message"], "launching");
        assert_eq!(line["span"], "vm.launch");
        assert_eq!(line["vm_id"], "VM-1");
        assert_eq!(line["level"], "INFO");
        assert_eq!(line["trace_id"].as_str().unwrap().len(), 32);
        assert_eq!(line["span_id"].as_str().unwrap().len(), 16);
    }

    #[test]
    fn a_log_line_outside_any_span_omits_the_trace_context() {
        let capture = Capture::default();
        let subscriber = tracing_subscriber::registry().with(
            fmt::layer()
                .event_format(WiabJson::new(Arc::new(OnceLock::new())))
                .with_writer(capture.clone()),
        );
        with_default(subscriber, || tracing::info!("plain"));

        let lines = capture.lines();
        let line = lines.last().expect("one line");
        assert_eq!(line["message"], "plain");
        assert!(line.get("trace_id").is_none());
        assert!(line.get("span").is_none());
    }

    #[test]
    fn audit_events_bypass_rust_log_and_land_on_the_audit_layer_only() {
        // The same layer shapes init() builds: debug filtered by an
        // error-level EnvFilter and never audit; audit filtered only by target.
        let debug_capture = Capture::default();
        let audit_capture = Capture::default();
        let subscriber = tracing_subscriber::registry()
            .with(
                fmt::layer()
                    .event_format(WiabJson::new(Arc::new(OnceLock::new())))
                    .with_writer(debug_capture.clone())
                    .with_filter(EnvFilter::new("error"))
                    .with_filter(filter::filter_fn(|metadata| {
                        metadata.target() != AUDIT_TARGET
                    })),
            )
            .with(
                fmt::layer()
                    .event_format(WiabJson::new(Arc::new(OnceLock::new())))
                    .with_writer(audit_capture.clone())
                    .with_filter(filter::filter_fn(|metadata| {
                        metadata.target() == AUDIT_TARGET
                    })),
            );
        with_default(subscriber, || {
            tracing::info!(target: "audit", event = "auth.login", outcome = "failure", "login");
            tracing::info!("ordinary info, below the error filter");
        });

        let audit_lines = audit_capture.lines();
        assert_eq!(audit_lines.len(), 1, "RUST_LOG=error cannot mute audit");
        assert_eq!(audit_lines[0]["stream"], "audit");
        assert_eq!(audit_lines[0]["event"], "auth.login");
        assert_eq!(audit_lines[0]["outcome"], "failure");
        assert!(
            debug_capture.lines().is_empty(),
            "audit and info both stay off the debug layer"
        );
    }
}
