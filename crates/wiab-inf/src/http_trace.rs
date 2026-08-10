//! HTTP server telemetry: one span and one duration sample per request.
//!
//! Wired outermost in the router so responses the inner layers generate
//! themselves — the auth gate's 401, the body limit's 413 — are measured too.
//! The route template from [`MatchedPath`] keeps cardinality bounded; the
//! trace id doubles as the request correlation id, so there is no separate
//! request-id machinery.

use std::collections::HashMap;
use std::time::Instant;

use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use opentelemetry::KeyValue;
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

pub async fn trace_http(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    // Router-level middleware runs after routing, so the matched template is
    // in the extensions; a fallback (404) has none.
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|path| path.as_str().to_owned());

    let span = tracing::info_span!(
        "http.request",
        otel.name = span_name(method.as_str(), route.as_deref()),
        otel.kind = "server",
        otel.status_code = tracing::field::Empty,
        http.request.method = %method,
        http.route = tracing::field::Empty,
        http.response.status_code = tracing::field::Empty,
        // Deliberately no url.query: OIDC callbacks carry codes there.
    );
    if let Some(route) = &route {
        span.record("http.route", route.as_str());
    }
    if let Some(carrier) = trace_headers(&request) {
        // A valid inbound traceparent continues the caller's trace; anything
        // malformed extracts to an invalid context, which set_parent treats
        // as "start a new root".
        let _ = span.set_parent(wiab_telemetry::extract_context(&carrier));
    }

    let started = Instant::now();
    let response = next.run(request).instrument(span.clone()).await;

    let status = response.status();
    span.record("http.response.status_code", status.as_u16());
    if status.is_server_error() {
        span.record("otel.status_code", "ERROR");
    }
    wiab_telemetry::metrics().http_request_duration.record(
        started.elapsed().as_secs_f64(),
        &[
            KeyValue::new("http.request.method", method.to_string()),
            KeyValue::new(
                "http.route",
                route.unwrap_or_else(|| "unmatched".to_owned()),
            ),
            KeyValue::new("http.response.status_code", i64::from(status.as_u16())),
        ],
    );
    response
}

fn span_name(method: &str, route: Option<&str>) -> String {
    format!("{method} {}", route.unwrap_or("unmatched"))
}

fn trace_headers(request: &Request) -> Option<HashMap<String, String>> {
    let mut carrier = HashMap::new();
    for name in ["traceparent", "tracestate"] {
        if let Some(value) = request.headers().get(name).and_then(|v| v.to_str().ok()) {
            carrier.insert(name.to_owned(), value.to_owned());
        }
    }
    (!carrier.is_empty()).then_some(carrier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_span_name_is_method_and_route_template() {
        assert_eq!(
            span_name("GET", Some("/repos/{repo_id}/git-upload-pack")),
            "GET /repos/{repo_id}/git-upload-pack"
        );
        assert_eq!(span_name("GET", None), "GET unmatched");
    }
}
