//! W3C trace-context propagation across process boundaries: inbound HTTP,
//! NATS headers, and the environment handed to launched containers.

use std::collections::HashMap;

use opentelemetry::trace::TraceContextExt;
use opentelemetry::{Context, global};
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// The current span's context as a W3C `traceparent` header value, or `None`
/// when there is no valid context to propagate.
pub fn current_traceparent() -> Option<String> {
    let context = tracing::Span::current().context();
    let span = context.span();
    let span_context = span.span_context();
    span_context
        .is_valid()
        .then(|| format_traceparent(span_context))
}

/// Extract a parent [`Context`] from carrier headers (`traceparent`,
/// `tracestate`). Invalid or absent headers yield an empty context, which
/// parents a new root — never an error.
pub fn extract_context(headers: &HashMap<String, String>) -> Context {
    global::get_text_map_propagator(|propagator| propagator.extract(headers))
}

fn format_traceparent(span_context: &opentelemetry::trace::SpanContext) -> String {
    format!(
        "00-{}-{}-{:02x}",
        span_context.trace_id(),
        span_context.span_id(),
        span_context.trace_flags().to_u8(),
    )
}

#[cfg(test)]
mod tests {
    use opentelemetry::trace::{SpanContext, SpanId, TraceFlags, TraceId, TraceState};
    use opentelemetry_sdk::propagation::TraceContextPropagator;

    use super::*;

    const TRACEPARENT: &str = "00-11111111111111111111111111111111-2222222222222222-01";

    #[test]
    fn a_span_context_formats_as_a_w3c_traceparent() {
        let span_context = SpanContext::new(
            TraceId::from_hex("11111111111111111111111111111111").unwrap(),
            SpanId::from_hex("2222222222222222").unwrap(),
            TraceFlags::SAMPLED,
            false,
            TraceState::default(),
        );
        assert_eq!(format_traceparent(&span_context), TRACEPARENT);
    }

    #[test]
    fn extraction_round_trips_through_headers() {
        global::set_text_map_propagator(TraceContextPropagator::new());
        let headers = HashMap::from([("traceparent".to_owned(), TRACEPARENT.to_owned())]);
        let context = extract_context(&headers);
        let binding = context.span();
        let span_context = binding.span_context();
        assert!(span_context.is_valid());
        assert_eq!(
            span_context.trace_id().to_string(),
            "11111111111111111111111111111111"
        );
    }

    #[test]
    fn garbage_headers_yield_an_invalid_context_not_an_error() {
        global::set_text_map_propagator(TraceContextPropagator::new());
        let headers = HashMap::from([("traceparent".to_owned(), "not-a-traceparent".to_owned())]);
        let context = extract_context(&headers);
        assert!(!context.span().span_context().is_valid());
    }
}
