# backend

The main process of workinabox. Contains everything to run agent farms and interact with them.

## Overview

The backend is a Rust service built on Tokio and Axum. It currently provides:

- an HTTP health endpoint
- meeting discovery
- a WebSocket signaling endpoint
- a mediasoup-based SFU for real-time audio
- optional local speech-to-text using Whisper

## Running locally

To run the full stack (Postgres + backend + frontend) together with Docker Compose, see
[`dev/local/README.md`](../dev/local/README.md) in the sibling `dev` repo:

```sh
docker compose -f dev/local/docker-compose.yml up
```

For backend-only iteration against a Dockerized Postgres, [`scripts/run-pg.sh`](scripts/run-pg.sh)
starts the database in Docker and runs the backend on the host with `cargo run`.

## Environment variables

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
