//! Telemetry environment resolution, next to the component that consumes it —
//! the same convention as `FirecrackerConfig::from_env` and friends.

/// The default `RUST_LOG` filter. Lists the actual crate targets: the historic
/// default (`wiab=info,tower_http=info`) named a crate that is not a
/// dependency and missed `wiab_inf`/`wiab_app`/`authbox_*` entirely.
pub const DEFAULT_FILTER: &str =
    "wiab=info,wiab_app=info,wiab_inf=info,authbox_app=info,authbox_inf=info";

/// Telemetry configuration, resolved from standard `OTEL_*` variables plus one
/// `WIAB_`-prefixed dev flag.
pub struct TelemetryConfig {
    /// Filter for debug telemetry (`RUST_LOG`). The audit stream is exempt.
    pub rust_log: String,
    /// OTLP http/protobuf base endpoint, e.g. `http://localhost:4318`
    /// (`OTEL_EXPORTER_OTLP_ENDPOINT`). Unset means no export anywhere.
    pub otlp_endpoint: Option<String>,
    /// `service.name` resource attribute (`OTEL_SERVICE_NAME`).
    pub service_name: String,
    /// Dev flag (`WIAB_OTEL_CONSOLE`): additionally dump spans and metrics to
    /// stdout without any collector.
    pub console: bool,
}

impl TelemetryConfig {
    pub fn from_env() -> Self {
        Self {
            rust_log: env_or("RUST_LOG", DEFAULT_FILTER),
            otlp_endpoint: std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            service_name: env_or("OTEL_SERVICE_NAME", "wiab"),
            console: env_flag("WIAB_OTEL_CONSOLE"),
        }
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_filter_names_the_real_crates() {
        for target in ["wiab", "wiab_app", "wiab_inf", "authbox_app", "authbox_inf"] {
            assert!(DEFAULT_FILTER.contains(target), "missing {target}");
        }
        assert!(!DEFAULT_FILTER.contains("tower_http"));
    }
}
