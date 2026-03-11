# backend

The main process of workinabox. Contains everything to run agent farms and interact with them.

## Overview

The backend is a Rust service built on Tokio and Axum. It currently provides:

- an HTTP health endpoint
- room discovery
- a WebSocket signaling endpoint
- a mediasoup-based SFU for real-time audio
- optional local speech-to-text using Whisper

## Environment variables

### Speech-to-text (optional)

Transcription is disabled by default. Setting `WIAB_WHISPER_MODEL_PATH` to a valid model file enables it.

#### Getting a model

Download a GGML-format Whisper model from [Hugging Face — ggerganov/whisper.cpp](https://huggingface.co/ggerganov/whisper.cpp/tree/main).
`ggml-base.en.bin` is a good starting point (≈ 142 MB, English only). For multilingual use `ggml-base.bin`.

```sh
mkdir -p ~/Models
curl -L -o ~/Models/ggml-base.en.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin
```

#### Setting the env var

Add to `~/.bashrc`, `~/.zshrc`, or equivalent:

```sh
export WIAB_WHISPER_MODEL_PATH="$HOME/Models/ggml-base.en.bin"
```

| Variable | Default | Description |
| --- | --- | --- |
| `WIAB_WHISPER_MODEL_PATH` | _(unset — transcription off)_ | Absolute path to a GGML Whisper model file |
| `WIAB_STT_LANGUAGE` | _(unset — auto-detect)_ | BCP-47 language code passed to Whisper, e.g. `en` |
| `WIAB_STT_THREADS` | `4` | Number of CPU threads for the Whisper inference worker |

### Networking

| Variable | Default | Description |
| --- | --- | --- |
| `WIAB_MEDIASOUP_LISTEN_IP` | `0.0.0.0` | IP mediasoup binds its WebRTC transports to |
| `WIAB_MEDIASOUP_ANNOUNCED_ADDRESS` | `10.0.2.2` | IP announced in ICE candidates (set to your public/LAN IP) |
