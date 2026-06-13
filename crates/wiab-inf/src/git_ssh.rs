//! Git SSH transport.
//!
//! Hosts an SSH server (on its own port) that, on a `git-upload-pack` /
//! `git-receive-pack` exec request, spawns the system `git`'s stateful pack process
//! against the bare repo on disk and pumps bytes between the SSH channel and the
//! subprocess.
//!
//! Auth is public-key only (like GitHub): the offered key's fingerprint must resolve to a
//! registered user, and that user's role on the target repo is checked at exec time
//! (Read for clone/fetch, Write for push). `none`/`password` are rejected. Anonymous
//! read of a public repo is served over HTTPS, not SSH.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use russh::keys::ssh_key::{HashAlg, PublicKey};
use russh::keys::{Algorithm, PrivateKey};
use russh::server::{Auth, Handler, Msg, Server, Session};
use russh::{Channel, ChannelId, MethodKind, MethodSet};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{ChildStdin, Command};
use tracing::{info, warn};
use wiab_core::access::Operation;
use wiab_core::repo::RepoId;
use wiab_core::user::UserId;

use crate::AppState;

/// Starts the git SSH server. Loads the host key from `host_key_path` if set, otherwise
/// generates an ephemeral one (fine for dev; clients see a changing host key).
pub async fn spawn_git_ssh_server(
    state: AppState,
    addr: String,
    host_key_path: Option<String>,
) -> anyhow::Result<()> {
    let host_key = match host_key_path {
        Some(path) => russh::keys::load_secret_key(&path, None)
            .map_err(|err| anyhow::anyhow!("failed to load SSH host key {path}: {err}"))?,
        None => {
            warn!("WIAB_GIT_SSH_HOST_KEY unset; generating an ephemeral SSH host key");
            PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519)?
        }
    };

    let config = Arc::new(russh::server::Config {
        methods: MethodSet::from(&[MethodKind::PublicKey][..]),
        keys: vec![host_key],
        ..Default::default()
    });

    let socket: SocketAddr = addr
        .parse()
        .map_err(|err| anyhow::anyhow!("invalid WIAB_GIT_SSH_ADDR '{addr}': {err}"))?;

    info!("git SSH transport listening on ssh://{socket}");
    let mut server = GitSshServer { state };
    server.run_on_address(config, socket).await?;
    Ok(())
}

struct GitSshServer {
    state: AppState,
}

impl Server for GitSshServer {
    type Handler = GitSshHandler;

    fn new_client(&mut self, _peer: Option<SocketAddr>) -> GitSshHandler {
        GitSshHandler {
            state: self.state.clone(),
            user: None,
            stdin: None,
        }
    }
}

struct GitSshHandler {
    state: AppState,
    /// The user resolved from the SSH public key at auth time; authorized per-repo at exec.
    user: Option<UserId>,
    /// Stdin of the running git subprocess; fed by incoming channel data.
    stdin: Option<ChildStdin>,
}

impl Handler for GitSshHandler {
    type Error = russh::Error;

    // SSH is key-only: the offered key's fingerprint must resolve to a registered user.
    // `none`/`password` use the trait defaults, which reject.
    async fn auth_publickey(&mut self, _user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();
        match self
            .state
            .user_service
            .resolve_user_by_fingerprint(&fingerprint)
            .await
        {
            Ok(Some(user)) => {
                self.user = Some(user);
                Ok(Auth::Accept)
            }
            Ok(None) | Err(_) => Ok(Auth::reject()),
        }
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let command = String::from_utf8_lossy(data);
        let Some((verb, repo_arg)) = parse_git_command(&command) else {
            return fail(session, channel, "unsupported command").await;
        };

        let Some(id) = parse_repo_id(&repo_arg) else {
            return fail(session, channel, "repo not found").await;
        };

        let path = self.state.git_root.join(format!("{id}.git"));
        if !path.exists() {
            return fail(session, channel, "repo not found").await;
        }

        let operation = if verb == "receive-pack" {
            Operation::Write
        } else {
            Operation::Read
        };
        if !self.authorize(id, operation).await {
            return fail(session, channel, "insufficient permissions").await;
        }

        self.spawn_git(channel, verb, &path, session).await
    }

    async fn data(
        &mut self,
        _channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(stdin) = self.stdin.as_mut() {
            let _ = stdin.write_all(data).await;
        }
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        _channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Client finished sending: close the subprocess's stdin so it can complete.
        if let Some(mut stdin) = self.stdin.take() {
            let _ = stdin.shutdown().await;
        }
        Ok(())
    }
}

impl GitSshHandler {
    /// Whether the key-authenticated user may perform `operation` on `repo`. SSH carries
    /// no token, so there is no scope cap.
    async fn authorize(&self, repo: RepoId, operation: Operation) -> bool {
        match self.user {
            Some(user) => self
                .state
                .authorization_service
                .authorize(user, repo, operation, None)
                .await
                .unwrap_or(false),
            None => false,
        }
    }

    async fn spawn_git(
        &mut self,
        channel: ChannelId,
        verb: &str,
        path: &PathBuf,
        session: &mut Session,
    ) -> Result<(), russh::Error> {
        let mut child = match Command::new("git")
            .arg(verb)
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(err) => return fail(session, channel, &format!("failed to run git: {err}")).await,
        };

        self.stdin = child.stdin.take();
        let mut stdout = child.stdout.take().expect("stdout piped");
        let mut stderr = child.stderr.take().expect("stderr piped");
        let handle = session.handle();

        // Pump subprocess output back to the client, then report exit and close.
        tokio::spawn(async move {
            let mut buf = [0u8; 16 * 1024];
            loop {
                match stdout.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if handle.data(channel, buf[..n].to_vec()).await.is_err() {
                            break;
                        }
                    }
                }
            }

            // Forward any diagnostics git wrote to stderr as SSH extended data.
            let mut err_buf = Vec::new();
            let _ = stderr.read_to_end(&mut err_buf).await;
            if !err_buf.is_empty() {
                let _ = handle.extended_data(channel, 1, err_buf).await;
            }

            let code = child
                .wait()
                .await
                .ok()
                .and_then(|status| status.code())
                .unwrap_or(0) as u32;
            let _ = handle.exit_status_request(channel, code).await;
            let _ = handle.eof(channel).await;
            let _ = handle.close(channel).await;
        });

        Ok(())
    }
}

/// Sends an error message on stderr, a non-zero exit, and closes the channel.
async fn fail(
    session: &mut Session,
    channel: ChannelId,
    message: &str,
) -> Result<(), russh::Error> {
    let handle = session.handle();
    let _ = handle
        .extended_data(channel, 1, format!("{message}\n").into_bytes())
        .await;
    let _ = handle.exit_status_request(channel, 1).await;
    let _ = handle.eof(channel).await;
    let _ = handle.close(channel).await;
    Ok(())
}

/// Parses a git SSH exec command (`git-upload-pack '/R-1.git'`) into (verb, repo arg).
fn parse_git_command(command: &str) -> Option<(&'static str, String)> {
    let command = command.trim();
    for (prefix, verb) in [
        ("git-upload-pack", "upload-pack"),
        ("git-receive-pack", "receive-pack"),
    ] {
        if let Some(rest) = command.strip_prefix(prefix) {
            let arg = rest.trim().trim_matches(['\'', '"']);
            return Some((verb, arg.to_owned()));
        }
    }
    None
}

/// Resolves the repo id from an SSH path argument like `/R-1.git` or `R-1`.
fn parse_repo_id(repo_arg: &str) -> Option<RepoId> {
    let trimmed = repo_arg.trim_start_matches('/');
    let trimmed = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    trimmed.parse::<RepoId>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_upload_and_receive_commands() {
        assert_eq!(
            parse_git_command("git-upload-pack '/R-1.git'"),
            Some(("upload-pack", "/R-1.git".to_owned()))
        );
        assert_eq!(
            parse_git_command("git-receive-pack '/R-12.git'"),
            Some(("receive-pack", "/R-12.git".to_owned()))
        );
        assert!(parse_git_command("rm -rf /").is_none());
    }

    #[test]
    fn resolves_repo_id_from_ssh_path() {
        assert_eq!(parse_repo_id("/R-1.git"), Some(RepoId::from_number(1)));
        assert_eq!(parse_repo_id("R-7"), Some(RepoId::from_number(7)));
        assert_eq!(parse_repo_id("/nope"), None);
    }
}
