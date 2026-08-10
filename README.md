# backend

The main process of workinabox — package `wiab`. See
[`docs/OVERVIEW.md`](../docs/OVERVIEW.md) for how it fits the wider system.

## Overview

A Rust service on Tokio and axum, structured as eight crates in two DDD families
(`wiab-{core,app,inf}` and the product-neutral `authbox-{core,app,inf}` identity
kit) plus `wiab-telemetry` and the in-guest `wiab-agent`. It provides:

- **Identity & access** (`authbox`): local password login (argon2id) with
  server-side sessions, Google + enterprise OIDC/SSO, PATs and SSH keys, invite,
  reset, verify; role/scope authorization (`Read<Write<Admin<Owner` over
  `Org⊇Project⊇Repo`).
- **Git hosting**: a bare repo per `Repo` aggregate, real `clone`/`fetch`/`push`
  over smart-HTTP and SSH, plus a REST browse/commit API.
- **Project model**: orgs, projects, repos, works (+ acceptance criteria), a
  7-state task lifecycle, boards, agents, and teams.
- **Agent execution**: teams that claim tasks and run in VM sandboxes
  (Firecracker on KVM, else Docker); two runtimes (Rust `wiab-agent` default,
  Python LangGraph via `WIAB_AGENT_RUNTIME=langgraph`).
- **Meetings**: a WebSocket signaling endpoint + mediasoup SFU, local Whisper
  STT, and llama-driven agent replies and minutes. (No working client today —
  see OVERVIEW.)
- **Messaging**: a transactional outbox published to NATS.
- **Telemetry**: OpenTelemetry traces/metrics/logs and an audit stream.
- **Health**: `GET /health`.

## Running locally

To run the full stack (Postgres + backend + frontend) together with Docker Compose, see
[`dev/local/README.md`](../dev/local/README.md) in the sibling `dev` repo:

```sh
docker compose -f dev/local/docker-compose.yml up
```

For backend-only iteration against a Dockerized Postgres, [`scripts/run-pg.sh`](scripts/run-pg.sh)
starts the database in Docker and runs the backend on the host with `cargo run`.

## Environment variables

The backend reads ~78 `WIAB_*`/`OTEL_*`/`RUST_LOG` variables. The **canonical,
complete inventory** is the code: `src/config.rs` (resolved once at startup) plus
the component `*::from_env()` sites in `wiab-inf` (Firecracker, Docker, NATS,
Llama, Whisper, media) and `wiab-telemetry/src/config.rs`. This README documents
only the load-bearing and commonly-set ones; when in doubt, read `config.rs`.

### Core

| Variable | Default | Description |
| --- | --- | --- |
| `WIAB_PERSISTENCE` | `postgres` | `postgres` or `memory` (in-memory, for tests) |
| `DATABASE_URL` | `postgres://wiab:wiab@localhost:5432/wiab` | Postgres connection (when persistence is `postgres`) |
| `WIAB_BASE_URL` | `http://localhost:3000` | Public base URL; drives cookie `Secure` and OIDC redirects |
| `WIAB_DEV_OWNER_PASSWORD` | _(unset)_ | Seeds the bootstrap owner. **Required for a non-local `WIAB_BASE_URL` — the backend refuses to start without it there** |
| `WIAB_TLS_CERT` / `WIAB_TLS_KEY` | _(unset)_ | PEM paths; both unset ⇒ a self-signed cert (the backend always serves HTTPS) |
| `WIAB_GIT_ROOT` | temp dir | Directory holding the bare git repos |
| `WIAB_GIT_SSH_ADDR` | `0.0.0.0:2222` | git-SSH transport bind address |
| `RUST_LOG` | `wiab=info,wiab_app=info,wiab_inf=info,authbox_app=info,authbox_inf=info` | Tracing filter (the audit stream is exempt) |

Auth/SSO (`WIAB_AUTH_*`, `WIAB_GOOGLE_*`, `WIAB_OIDC_*`), email
(`WIAB_EMAIL_*`, `RESEND_API_KEY`, `WIAB_SMTP_*`), messaging (`WIAB_NATS_*`),
the agent runtime (`WIAB_AGENT_RUNTIME` + `WIAB_TEAM_*`), and the VM runtimes
(`WIAB_FIRECRACKER_*`, `WIAB_JAIL*`, `WIAB_DOCKER_*`) are all resolved in
`config.rs` / the `from_env()` sites — see there for the full set.

### Telemetry (operator)

Off by default: JSON logs to stdout with `trace_id`/`span_id`, an always-on
`audit` stream on stdout (never suppressed by `RUST_LOG`), spans created but not
exported, metrics off. To turn export on, set `OTEL_EXPORTER_OTLP_ENDPOINT`
(http/protobuf, e.g. `http://collector:4318`) — traces, metrics, and logs then
export via OTLP. `OTEL_SERVICE_NAME` (default `wiab`) and
`OTEL_RESOURCE_ATTRIBUTES` set resource attributes; `WIAB_OTEL_CONSOLE=1` dumps
spans/metrics to stdout for local debugging without a collector. What was
deliberately deferred: [`docs/TELEMETRY_FOLLOWUP.md`](../docs/TELEMETRY_FOLLOWUP.md).

### Local models (Llama LLM, Whisper STT)

Both local models are disabled by default and loaded eagerly at startup. Each is toggled by an
enable flag; when a model is enabled its file must be present or the backend aborts startup.

- **Llama** powers meeting intelligence (agent replies + minutes). Disabled ⇒ no agent replies or minutes.
- **Whisper** powers real-time transcription. Disabled ⇒ no speech-to-text.

Model files live under `${WIAB_DATA_DIR}/models/<filename>`. `WIAB_DATA_DIR` defaults to
`~/.local/share/wiab` locally (mirrors the production `/var/lib/wiab`). The per-model variables
hold the **filename only**, resolved against that directory.

| Variable | Default | Description |
| --- | --- | --- |
| `WIAB_DATA_DIR` | `~/.local/share/wiab` | Base data dir; models load from `<dir>/models` |
| `WIAB_LLAMA_ENABLED` | `false` | Enable the Llama meeting-intelligence model |
| `WIAB_LLAMA_MODEL_FILE` | _(unset)_ | Llama model filename, e.g. `gemma-3-1b-it-Q4_K_M.gguf` |
| `WIAB_WHISPER_ENABLED` | `false` | Enable Whisper transcription |
| `WIAB_WHISPER_MODEL_FILE` | _(unset)_ | Whisper model filename, e.g. `ggml-base.en.bin` |
| `WIAB_STT_LANGUAGE` | _(unset — auto-detect)_ | BCP-47 language code passed to Whisper, e.g. `en` |
| `WIAB_STT_THREADS` | `4` | CPU threads for the Whisper inference worker |

The Llama loader also accepts optional tuning vars: `WIAB_LLAMA_CONTEXT_TOKENS`,
`WIAB_LLAMA_MAX_REPLY_TOKENS`, `WIAB_LLAMA_MAX_MINUTES_TOKENS`, `WIAB_LLAMA_THREADS`,
`WIAB_LLAMA_N_GPU_LAYERS`, `WIAB_LLAMA_CHAT_TEMPLATE`.

#### Getting the model files

Model files are stored in Azure blob storage and fetched with `azcopy`. Set `WIAB_MODELS_URL`
to the container URL (with a SAS token) and run the helper, which downloads each enabled model
into `${WIAB_DATA_DIR}/models`:

```sh
export WIAB_MODELS_URL="https://<acct>.blob.core.windows.net/<container>?<SAS>"
export WIAB_LLAMA_ENABLED=true   WIAB_LLAMA_MODEL_FILE=gemma-3-1b-it-Q4_K_M.gguf
export WIAB_WHISPER_ENABLED=true WIAB_WHISPER_MODEL_FILE=ggml-base.en.bin
backend/scripts/fetch-models.sh   # macOS: brew install azcopy
```

A GGML-format Whisper model such as `ggml-base.en.bin` (≈ 142 MB, English only) comes from
[Hugging Face — ggerganov/whisper.cpp](https://huggingface.co/ggerganov/whisper.cpp/tree/main)
if you need to (re)populate the Azure container.

### Networking

| Variable | Default | Description |
| --- | --- | --- |
| `WIAB_MEDIASOUP_LISTEN_IP` | `0.0.0.0` | IP mediasoup binds its WebRTC transports to |
| `WIAB_MEDIASOUP_ANNOUNCED_ADDRESS` | `10.0.2.2` | IP announced in ICE candidates (set to your public/LAN IP) |
