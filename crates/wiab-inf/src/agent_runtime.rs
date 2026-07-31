//! Which agent runs inside a sandbox.
//!
//! Two implementations exist: the Rust `wiab-agent` binary (the default), and the
//! Python/LangGraph dev team from the `sw-dev-team` repo. Both run in the same
//! sandbox the VM runtimes already provision; only the image and the environment
//! handed to the guest differ, so the selection lives here rather than in the
//! domain or the `VmSpec`.

use std::str::FromStr;

/// Env var selecting the agent runtime. Unset or unrecognised means `rust`.
pub const AGENT_RUNTIME_ENV: &str = "WIAB_AGENT_RUNTIME";

/// Prefix of the environment forwarded to a LangGraph guest. Every var the agent
/// team reads lives in this namespace (see `sw-dev-team/docs/ENV.md`), so
/// forwarding by prefix keeps the two sides from drifting apart as options are
/// added.
const TEAM_ENV_PREFIX: &str = "WIAB_TEAM_";

/// The model credential the agent team needs. Outside the `WIAB_TEAM_` namespace
/// because it is the standard Anthropic variable name.
const MODEL_API_KEY_ENV: &str = "ANTHROPIC_API_KEY";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentRuntime {
    /// The Rust `wiab-agent` binary.
    #[default]
    Rust,
    /// The Python/LangGraph agent dev team.
    LangGraph,
}

impl AgentRuntime {
    /// Read the selection from the environment. An unrecognised value falls back
    /// to the default and warns, rather than failing a boot over a typo.
    pub fn from_env() -> Self {
        match std::env::var(AGENT_RUNTIME_ENV) {
            Ok(value) if !value.is_empty() => value.parse().unwrap_or_else(|_| {
                tracing::warn!(
                    "{AGENT_RUNTIME_ENV}={value} is not a known agent runtime; using rust"
                );
                Self::Rust
            }),
            _ => Self::Rust,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::LangGraph => "langgraph",
        }
    }

    /// Docker image prefix for this runtime. The two agents are baked into
    /// different images, so selecting a runtime selects an image family:
    /// `wiab-agent-<template>` or `wiab-team-<template>`.
    pub fn image_prefix(&self) -> &'static str {
        match self {
            Self::Rust => "wiab-agent-",
            Self::LangGraph => "wiab-team-",
        }
    }

    /// Whether the guest needs a writable workspace. The agent team clones a repo
    /// and creates a git worktree per developer; the Rust agent only writes to
    /// stdout, so it keeps a fully read-only rootfs.
    pub fn needs_workspace(&self) -> bool {
        matches!(self, Self::LangGraph)
    }
}

impl FromStr for AgentRuntime {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "rust" => Ok(Self::Rust),
            "langgraph" => Ok(Self::LangGraph),
            _ => Err(()),
        }
    }
}

/// Environment forwarded from the backend's own process into a LangGraph guest:
/// everything in the `WIAB_TEAM_` namespace plus the model credential.
///
/// Forwarding rather than enumerating means an option added to the agent team
/// needs no change here. Sorted so a launch is reproducible.
pub fn langgraph_env() -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = std::env::vars()
        .filter(|(key, _)| key.starts_with(TEAM_ENV_PREFIX) || key == MODEL_API_KEY_ENV)
        .collect();
    env.sort();
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_known_runtimes() {
        assert_eq!("rust".parse(), Ok(AgentRuntime::Rust));
        assert_eq!("langgraph".parse(), Ok(AgentRuntime::LangGraph));
    }

    #[test]
    fn parsing_ignores_case_and_surrounding_space() {
        assert_eq!(" LangGraph ".parse(), Ok(AgentRuntime::LangGraph));
    }

    #[test]
    fn rejects_an_unknown_runtime() {
        assert_eq!("python".parse::<AgentRuntime>(), Err(()));
    }

    #[test]
    fn defaults_to_the_rust_agent() {
        assert_eq!(AgentRuntime::default(), AgentRuntime::Rust);
    }

    #[test]
    fn each_runtime_has_its_own_image_family() {
        assert_ne!(
            AgentRuntime::Rust.image_prefix(),
            AgentRuntime::LangGraph.image_prefix()
        );
    }

    #[test]
    fn only_the_agent_team_needs_a_writable_workspace() {
        assert!(!AgentRuntime::Rust.needs_workspace());
        assert!(AgentRuntime::LangGraph.needs_workspace());
    }

    #[test]
    fn as_str_round_trips_through_from_str() {
        for runtime in [AgentRuntime::Rust, AgentRuntime::LangGraph] {
            assert_eq!(runtime.as_str().parse(), Ok(runtime));
        }
    }
}
