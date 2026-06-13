//! Git Smart-HTTP transport.
//!
//! libgit2 reads and writes objects but does not speak the pack protocol over the
//! wire, so real `git clone`/`fetch`/`push` are served by spawning the system `git`'s
//! `upload-pack` / `receive-pack` against the bare repo on disk. Clone/fetch
//! (`upload-pack`) is unauthenticated; push (`receive-pack`) requires the repo's push
//! token via HTTP Basic auth.

// Helpers return `Result<_, Response>` so handlers can `?` and emit the error directly;
// the large `Err` variant (an HTTP response) is intentional, not an oversight.
#![allow(clippy::result_large_err)]

use std::io::Read;
use std::path::PathBuf;
use std::process::Stdio;

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use wiab_core::access::Operation;
use wiab_core::repo::{RepoId, Visibility};

use crate::AppState;
use crate::http_api::basic_auth_password;

#[derive(serde::Deserialize)]
pub struct InfoRefsQuery {
    service: String,
}

/// `GET /repos/{repo}.git/info/refs?service=git-upload-pack|git-receive-pack`
pub async fn info_refs(
    State(state): State<AppState>,
    Path(repo_id_git): Path<String>,
    Query(query): Query<InfoRefsQuery>,
    headers: HeaderMap,
) -> Response {
    let service = query.service.as_str();
    if service != "git-upload-pack" && service != "git-receive-pack" {
        return (StatusCode::FORBIDDEN, "unsupported service").into_response();
    }

    let id = match parse_repo_id(&repo_id_git) {
        Ok(id) => id,
        Err(response) => return response,
    };

    // Advertising refs is gated by the same operation the client is about to perform.
    let operation = if service == "git-receive-pack" {
        Operation::Write
    } else {
        Operation::Read
    };
    if let Err(response) = authorize_git(&state, id, operation, &headers).await {
        return response;
    }

    let path = match bare_path(&state, &id) {
        Ok(path) => path,
        Err(response) => return response,
    };

    let verb = service.trim_start_matches("git-");
    let output = match Command::new("git")
        .arg(verb)
        .arg("--stateless-rpc")
        .arg("--advertise-refs")
        .arg(&path)
        .output()
        .await
    {
        Ok(output) if output.status.success() => output.stdout,
        Ok(output) => return spawn_failure(verb, &output.stderr),
        Err(err) => return spawn_error(err),
    };

    let mut body = pkt_line(&format!("# service={service}\n"));
    body.extend_from_slice(b"0000"); // flush packet
    body.extend_from_slice(&output);

    (
        [
            (
                header::CONTENT_TYPE,
                format!("application/x-{service}-advertisement"),
            ),
            (header::CACHE_CONTROL, "no-cache".to_owned()),
        ],
        body,
    )
        .into_response()
}

/// `POST /repos/{repo}.git/git-upload-pack` — clone/fetch (no auth).
pub async fn upload_pack(
    State(state): State<AppState>,
    Path(repo_id_git): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    rpc(
        &state,
        &repo_id_git,
        "git-upload-pack",
        &headers,
        body,
        Operation::Read,
    )
    .await
}

/// `POST /repos/{repo}.git/git-receive-pack` — push (push-token auth).
pub async fn receive_pack(
    State(state): State<AppState>,
    Path(repo_id_git): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    rpc(
        &state,
        &repo_id_git,
        "git-receive-pack",
        &headers,
        body,
        Operation::Write,
    )
    .await
}

async fn rpc(
    state: &AppState,
    repo_id_git: &str,
    service: &str,
    headers: &HeaderMap,
    body: Bytes,
    operation: Operation,
) -> Response {
    let id = match parse_repo_id(repo_id_git) {
        Ok(id) => id,
        Err(response) => return response,
    };
    if let Err(response) = authorize_git(state, id, operation, headers).await {
        return response;
    }
    let path = match bare_path(state, &id) {
        Ok(path) => path,
        Err(response) => return response,
    };

    let body = match decode_body(headers, body) {
        Ok(body) => body,
        Err(response) => return response,
    };

    let verb = service.trim_start_matches("git-");
    let mut child = match Command::new("git")
        .arg(verb)
        .arg("--stateless-rpc")
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => return spawn_error(err),
    };

    // Feed the request body concurrently with reading output to avoid a pipe-buffer
    // deadlock on large transfers.
    let mut stdin = child.stdin.take().expect("stdin piped");
    tokio::spawn(async move {
        let _ = stdin.write_all(&body).await;
        let _ = stdin.shutdown().await;
    });

    let output = match child.wait_with_output().await {
        Ok(output) if output.status.success() => output.stdout,
        Ok(output) => return spawn_failure(verb, &output.stderr),
        Err(err) => return spawn_error(err),
    };

    (
        [(
            header::CONTENT_TYPE,
            format!("application/x-{service}-result"),
        )],
        output,
    )
        .into_response()
}

/// Decompresses the request body when the client sent it gzip-encoded.
fn decode_body(headers: &HeaderMap, body: Bytes) -> Result<Vec<u8>, Response> {
    let gzipped = headers
        .get(header::CONTENT_ENCODING)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.eq_ignore_ascii_case("gzip"))
        .unwrap_or(false);
    if !gzipped {
        return Ok(body.to_vec());
    }
    let mut decoder = flate2::read::GzDecoder::new(&body[..]);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).map_err(|err| {
        (StatusCode::BAD_REQUEST, format!("invalid gzip body: {err}")).into_response()
    })?;
    Ok(out)
}

/// Authorizes a git operation over HTTPS: anonymous read of a public repo is allowed,
/// otherwise the request must present an access token whose user holds a sufficient role
/// (capped by the token's scope).
async fn authorize_git(
    state: &AppState,
    repo: RepoId,
    operation: Operation,
    headers: &HeaderMap,
) -> Result<(), Response> {
    if operation == Operation::Read {
        let rid = repo.to_string();
        let visibility = state
            .repo_service
            .repo_visibility(&rid)
            .await
            .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()).into_response())?;
        if visibility == Some(Visibility::Public) {
            return Ok(());
        }
    }

    let Some(token) = basic_auth_password(headers) else {
        return Err(unauthorized());
    };
    let Some((user, scope)) = state
        .user_service
        .resolve_token(&token)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response())?
    else {
        return Err(unauthorized());
    };

    let allowed = state
        .authorization_service
        .authorize(user, repo, operation, Some(&scope))
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response())?;
    if allowed { Ok(()) } else { Err(forbidden()) }
}

/// 401 carrying a Basic challenge so `git` retries with credentials.
fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Basic realm=\"wiab-git\"")],
        "authentication required",
    )
        .into_response()
}

/// 403 — authenticated, but the user/token lacks the required role.
fn forbidden() -> Response {
    (StatusCode::FORBIDDEN, "insufficient permissions").into_response()
}

fn parse_repo_id(repo_id_git: &str) -> Result<RepoId, Response> {
    let trimmed = repo_id_git.strip_suffix(".git").unwrap_or(repo_id_git);
    trimmed
        .parse::<RepoId>()
        .map_err(|_| not_found(repo_id_git))
}

fn bare_path(state: &AppState, id: &RepoId) -> Result<PathBuf, Response> {
    let path = state.git_root.join(format!("{id}.git"));
    if path.exists() {
        Ok(path)
    } else {
        Err(not_found(&id.to_string()))
    }
}

fn not_found(repo: &str) -> Response {
    (StatusCode::NOT_FOUND, format!("repo '{repo}' not found")).into_response()
}

fn spawn_error(err: std::io::Error) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("failed to run git: {err}"),
    )
        .into_response()
}

fn spawn_failure(verb: &str, stderr: &[u8]) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("git {verb} failed: {}", String::from_utf8_lossy(stderr)),
    )
        .into_response()
}

/// Encodes `payload` as a single pkt-line (4-hex length prefix + payload).
fn pkt_line(payload: &str) -> Vec<u8> {
    let length = payload.len() + 4;
    let mut out = format!("{length:04x}").into_bytes();
    out.extend_from_slice(payload.as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkt_line_prefixes_hex_length() {
        assert_eq!(pkt_line("a\n"), b"0006a\n".to_vec());
        assert_eq!(
            pkt_line("# service=git-upload-pack\n"),
            b"001e# service=git-upload-pack\n".to_vec()
        );
    }

    #[test]
    fn parse_repo_id_strips_dot_git() {
        assert_eq!(parse_repo_id("R-12.git").unwrap(), RepoId::from_number(12));
        assert_eq!(parse_repo_id("R-3").unwrap(), RepoId::from_number(3));
        assert!(parse_repo_id("nope").is_err());
    }
}
