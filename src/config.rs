//! Centralized application configuration.
//!
//! Every environment variable the backend reads is resolved here, once, at startup — instead of
//! scattered `std::env::var` calls across bootstrap and the infrastructure crate. `main` parses
//! the CLI, builds an [`AppConfig`], and threads it (and its sub-configs) into `build_app_state`
//! and the runtime components, which no longer touch the environment themselves.
//!
//! Grouped sub-configs keep each concern together. Structs consumed inside `wiab-inf`
//! (Firecracker, Docker, Llama, Whisper, media) live in that crate next to their components; the
//! groups consumed here in the binary (serve, auth, email, dev-seeding) live in this module.

use std::path::PathBuf;

use wiab_inf::{DockerConfig, FirecrackerConfig, LlamaConfig, MediaConfig, NatsConfig};

use crate::Cli;

/// The whole backend configuration, resolved once at startup.
pub struct AppConfig {
    pub serve: ServeConfig,
    pub vm: VmConfig,
    /// Message broker. `None` unless `WIAB_NATS_ENABLED` is set.
    pub nats: Option<NatsConfig>,
    pub auth: AuthConfig,
    pub email: EmailConfig,
    pub dev: DevConfig,
    pub meeting: MeetingConfig,
    pub media: MediaConfig,
}

/// Process/serving config: persistence selection, the addresses we bind, and TLS material.
pub struct ServeConfig {
    /// Persistence backend: `postgres` or `memory` (from `--persistence` / `WIAB_PERSISTENCE`).
    pub persistence: String,
    /// Postgres connection URL (from `--database-url` / `DATABASE_URL`).
    pub database_url: String,
    /// Address the git SSH transport binds (`WIAB_GIT_SSH_ADDR`).
    pub git_ssh_addr: String,
    /// Optional git SSH host key path (`WIAB_GIT_SSH_HOST_KEY`).
    pub git_ssh_host_key: Option<String>,
    /// TLS cert/key PEM paths (`WIAB_TLS_CERT`/`WIAB_TLS_KEY`); both unset → self-signed.
    pub tls_cert: Option<String>,
    pub tls_key: Option<String>,
    /// Directory hosting git repos (`WIAB_GIT_ROOT`, else a temp dir).
    pub git_root: PathBuf,
    /// Tracing filter (`RUST_LOG`).
    pub rust_log: String,
}

/// VM runtime selection and per-backend config. Firecracker is used only when enabled AND
/// `/dev/kvm` is present (checked at bootstrap); otherwise the Docker backend runs.
pub struct VmConfig {
    /// `WIAB_FIRECRACKER_ENABLED` — half the Firecracker decision (the other half is `/dev/kvm`).
    pub firecracker_enabled: bool,
    pub firecracker: FirecrackerConfig,
    pub docker: DockerConfig,
}

/// HTTP auth + inbound SSO/OIDC federation. Client secrets default to empty (the feature is
/// off unless its `*_ENABLED` flag is set), matching the previous inline behavior.
pub struct AuthConfig {
    /// Public base URL (`WIAB_BASE_URL`); drives cookie `Secure` and OIDC redirect URIs.
    pub base_url: String,
    pub local_signup: bool,
    pub google_enabled: bool,
    pub oidc_enabled: bool,
    pub google_client_id: String,
    pub google_client_secret: String,
    pub oidc_issuer: String,
    pub oidc_client_id: String,
    pub oidc_client_secret: String,
}

/// Transactional email delivery. Missing credentials fall back to logging (handled in bootstrap).
pub struct EmailConfig {
    pub from: String,
    /// `resend` (default) or `smtp`.
    pub provider: String,
    pub resend_api_key: Option<String>,
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_user: Option<String>,
    pub smtp_password: Option<String>,
    pub smtp_tls: bool,
}

/// Dev-only seeding conveniences (owner password + optional pre-provisioned SSO owner).
pub struct DevConfig {
    /// `WIAB_DEV_OWNER_PASSWORD`. There is no default: a baked-in one is a published
    /// credential, and `seed_owner_password` refuses to invent one outside local development.
    pub owner_password: Option<String>,
    pub sso_owner_email: String,
    pub sso_owner_name: String,
    pub sso_owner_password: Option<String>,
}

/// Meeting-intelligence config. `llama` is `Some` only when `WIAB_LLAMA_ENABLED` is set, in
/// which case its model config is resolved and validated here; the model itself is loaded later
/// in `build_app_state`.
pub struct MeetingConfig {
    pub llama: Option<LlamaConfig>,
}

impl AppConfig {
    /// Resolve the full configuration from the parsed CLI plus the environment. Reads only —
    /// heavy work (DB connect, model loading) stays in `build_app_state`.
    pub fn load(cli: &Cli) -> anyhow::Result<Self> {
        Ok(Self {
            serve: ServeConfig {
                persistence: cli.persistence.clone(),
                database_url: cli.database_url.clone(),
                git_ssh_addr: env_or("WIAB_GIT_SSH_ADDR", "0.0.0.0:2222"),
                git_ssh_host_key: std::env::var("WIAB_GIT_SSH_HOST_KEY").ok(),
                tls_cert: std::env::var("WIAB_TLS_CERT").ok(),
                tls_key: std::env::var("WIAB_TLS_KEY").ok(),
                git_root: std::env::var("WIAB_GIT_ROOT")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| std::env::temp_dir().join("wiab-git")),
                rust_log: env_or("RUST_LOG", "wiab=info,tower_http=info"),
            },
            nats: if env_flag("WIAB_NATS_ENABLED") {
                Some(NatsConfig::from_env())
            } else {
                None
            },
            vm: VmConfig {
                firecracker_enabled: env_flag("WIAB_FIRECRACKER_ENABLED"),
                firecracker: FirecrackerConfig::from_env(),
                docker: DockerConfig::from_env(),
            },
            auth: AuthConfig {
                base_url: env_or("WIAB_BASE_URL", "http://localhost:3000"),
                local_signup: env_flag("WIAB_AUTH_LOCAL_SIGNUP"),
                google_enabled: env_flag("WIAB_AUTH_GOOGLE_ENABLED"),
                oidc_enabled: env_flag("WIAB_AUTH_OIDC_ENABLED"),
                google_client_id: std::env::var("WIAB_GOOGLE_CLIENT_ID").unwrap_or_default(),
                google_client_secret: std::env::var("WIAB_GOOGLE_CLIENT_SECRET")
                    .unwrap_or_default(),
                oidc_issuer: std::env::var("WIAB_OIDC_ISSUER").unwrap_or_default(),
                oidc_client_id: std::env::var("WIAB_OIDC_CLIENT_ID").unwrap_or_default(),
                oidc_client_secret: std::env::var("WIAB_OIDC_CLIENT_SECRET").unwrap_or_default(),
            },
            email: EmailConfig {
                from: std::env::var("WIAB_EMAIL_FROM")
                    .or_else(|_| std::env::var("WIAB_SMTP_FROM"))
                    .unwrap_or_else(|_| "no-reply@workinabox.local".to_owned()),
                provider: env_or("WIAB_EMAIL_PROVIDER", "resend"),
                resend_api_key: std::env::var("RESEND_API_KEY")
                    .ok()
                    .filter(|k| !k.trim().is_empty()),
                smtp_host: std::env::var("WIAB_SMTP_HOST")
                    .ok()
                    .filter(|h| !h.trim().is_empty()),
                smtp_port: std::env::var("WIAB_SMTP_PORT")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(587),
                smtp_user: std::env::var("WIAB_SMTP_USER")
                    .ok()
                    .filter(|s| !s.is_empty()),
                smtp_password: std::env::var("WIAB_SMTP_PASSWORD")
                    .ok()
                    .filter(|s| !s.is_empty()),
                smtp_tls: env_flag("WIAB_SMTP_TLS"),
            },
            dev: {
                let sso_owner_email = std::env::var("WIAB_DEV_SSO_OWNER_EMAIL").unwrap_or_default();
                let sso_owner_name = std::env::var("WIAB_DEV_SSO_OWNER_NAME")
                    .unwrap_or_else(|_| sso_owner_email.clone());
                DevConfig {
                    owner_password: std::env::var("WIAB_DEV_OWNER_PASSWORD")
                        .ok()
                        .filter(|p| !p.is_empty()),
                    sso_owner_email,
                    sso_owner_name,
                    sso_owner_password: std::env::var("WIAB_DEV_SSO_OWNER_PASSWORD")
                        .ok()
                        .filter(|p| !p.is_empty()),
                }
            },
            meeting: MeetingConfig {
                llama: if env_flag("WIAB_LLAMA_ENABLED") {
                    Some(LlamaConfig::from_env()?)
                } else {
                    None
                },
            },
            media: MediaConfig::from_env()?,
        })
    }
}

/// Read an env var, falling back to a default when unset.
fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

/// The password to seed the bootstrap Owner with, or an error refusing to invent one.
///
/// The convenience default this replaces was a published credential: any deployment that did
/// not set `WIAB_DEV_OWNER_PASSWORD` shipped with `owner/owner` as a full administrator. It is
/// only safe where nobody but the developer can reach the host, so it survives for a local
/// base URL and nowhere else.
pub fn seed_owner_password(
    configured: Option<&str>,
    base_url: &str,
) -> anyhow::Result<(String, bool)> {
    match configured {
        Some(password) => Ok((password.to_owned(), false)),
        None if is_local_base_url(base_url) => Ok((LOCAL_DEV_OWNER_PASSWORD.to_owned(), true)),
        None => anyhow::bail!(
            "refusing to seed the bootstrap owner with a default password for a non-local \
             deployment ({base_url}): set WIAB_DEV_OWNER_PASSWORD"
        ),
    }
}

/// Password used for the seeded owner when developing against a local base URL.
const LOCAL_DEV_OWNER_PASSWORD: &str = "owner";

/// Whether the advertised base URL points at the developer's own machine.
fn is_local_base_url(base_url: &str) -> bool {
    let host = base_url
        .split("://")
        .nth(1)
        .unwrap_or(base_url)
        .split('/')
        .next()
        .unwrap_or_default();
    let host = host.split(':').next().unwrap_or(host);
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]")
}

/// Reads a boolean env flag (`1`/`true`/`yes`/`on` = true), defaulting to false when unset.
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
    fn a_non_local_deployment_refuses_to_invent_an_owner_password() {
        let error = seed_owner_password(None, "https://wiab.example.com")
            .expect_err("a deployed backend must not seed a published default credential");
        assert!(
            error.to_string().contains("WIAB_DEV_OWNER_PASSWORD"),
            "the error should say how to fix it: {error}"
        );
    }

    #[test]
    fn local_development_keeps_its_default() {
        let (password, is_default) =
            seed_owner_password(None, "http://localhost:3000").expect("local dev is allowed");
        assert_eq!(password, LOCAL_DEV_OWNER_PASSWORD);
        assert!(is_default);
    }

    #[test]
    fn a_configured_password_is_used_everywhere() {
        for base_url in ["http://localhost:3000", "https://wiab.example.com"] {
            let (password, is_default) =
                seed_owner_password(Some("s3cret"), base_url).expect("an explicit password works");
            assert_eq!(password, "s3cret");
            assert!(!is_default);
        }
    }

    #[test]
    fn only_loopback_hosts_count_as_local() {
        assert!(is_local_base_url("http://localhost:3000"));
        assert!(is_local_base_url("http://127.0.0.1:8080/"));
        assert!(!is_local_base_url("https://wiab.example.com"));
        // A host that merely mentions localhost is not localhost.
        assert!(!is_local_base_url("https://localhost.evil.com"));
    }
}
