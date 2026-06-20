//! Shared location and env helpers for the local model loaders (Llama, Whisper).
//!
//! Model files live under `${WIAB_DATA_DIR}/models/<filename>`. The data dir mirrors the
//! `WIAB_GIT_ROOT` convention: an explicit env var in production (`/var/lib/wiab`), a user
//! directory locally. Files are fetched into this location at provision time (via azcopy);
//! the application only reads them and hard-fails if an enabled model is absent.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

/// Reads a boolean env flag (`1`/`true`/`yes`/`on` = true), defaulting to false when unset.
pub(crate) fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// Reads a required env var, erroring if it is unset or empty/whitespace-only.
pub(crate) fn required_env(key: &str) -> anyhow::Result<String> {
    let value = std::env::var(key).with_context(|| format!("missing required env var {key}"))?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("env var {key} must not be empty");
    }
    Ok(trimmed.to_owned())
}

/// Base data directory holding model files. `WIAB_DATA_DIR` when set, otherwise
/// `$HOME/.local/share/wiab` for local development. Never the temp dir — model files are
/// large and must survive restarts. Errors if neither is available.
pub(crate) fn data_dir() -> anyhow::Result<PathBuf> {
    if let Ok(dir) = std::env::var("WIAB_DATA_DIR") {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    let home = std::env::var("HOME")
        .ok()
        .map(|home| home.trim().to_owned())
        .filter(|home| !home.is_empty() && home != "/");
    match home {
        Some(home) => Ok(PathBuf::from(home).join(".local/share/wiab")),
        None => bail!(
            "WIAB_DATA_DIR is unset and HOME is unavailable; set WIAB_DATA_DIR to the model data directory"
        ),
    }
}

/// Resolve `${WIAB_DATA_DIR}/models/<filename>`, ensuring the models directory exists, and
/// erroring if the file is absent. Provides identical hard-fail semantics for both loaders.
pub(crate) fn resolve_model_file(filename: &str) -> anyhow::Result<PathBuf> {
    resolve_in(&data_dir()?, filename)
}

fn resolve_in(data_dir: &Path, filename: &str) -> anyhow::Result<PathBuf> {
    let models_dir = data_dir.join("models");
    std::fs::create_dir_all(&models_dir)
        .with_context(|| format!("failed to create models dir {}", models_dir.display()))?;
    let path = models_dir.join(filename);
    if !path.exists() {
        bail!(
            "model file '{}' not found at {} (fetch it at provision time via azcopy)",
            filename,
            path.display()
        );
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn resolve_errors_when_missing_then_returns_path_when_present() {
        let dir = tempdir().expect("tempdir");
        let base = dir.path();

        let err = resolve_in(base, "model.gguf").expect_err("missing file should error");
        assert!(err.to_string().contains("not found"));

        // create_dir_all ran, so the models dir now exists.
        let models_dir = base.join("models");
        assert!(models_dir.is_dir());

        fs::write(models_dir.join("model.gguf"), b"x").expect("write model file");
        let resolved = resolve_in(base, "model.gguf").expect("present file should resolve");
        assert_eq!(resolved, models_dir.join("model.gguf"));
    }
}
