#!/usr/bin/env bash
# Fetch the local model files (Llama LLM, Whisper STT) from Azure blob storage into the
# standard data dir so the backend can load them. Mirrors the deploy-time fetch in
# iac/scripts/wiab-deploy.sh. Requires azcopy on PATH (macOS: `brew install azcopy`).
#
# Config (env):
#   WIAB_MODELS_URL  Azure container URL WITH a SAS token, e.g.
#                    https://<acct>.blob.core.windows.net/<container>?<SAS>
#   WIAB_LLAMA_ENABLED=true   WIAB_LLAMA_MODEL_FILE=gemma-3-1b-it-Q4_K_M.gguf
#   WIAB_WHISPER_ENABLED=true WIAB_WHISPER_MODEL_FILE=ggml-base.en.bin
# Files land in ${WIAB_DATA_DIR:-$HOME/.local/share/wiab}/models.
set -euo pipefail

WIAB_DATA_DIR="${WIAB_DATA_DIR:-$HOME/.local/share/wiab}"
MODELS_DIR="${WIAB_DATA_DIR}/models"

is_enabled() {
  case "$(printf '%s' "${1:-}" | tr '[:upper:]' '[:lower:]')" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

# Roles are discovered generically from the WIAB_<ROLE>_MODEL_FILE vars in the environment,
# so any number of model slots works (same scan as iac/scripts/wiab-deploy.sh).
roles_with_files() { compgen -A variable | grep -E '^WIAB_[A-Z0-9]+_MODEL_FILE$' || true; }

any_enabled=0
for mfvar in $(roles_with_files); do
  role="${mfvar#WIAB_}"; role="${role%_MODEL_FILE}"
  efvar="WIAB_${role}_ENABLED"
  is_enabled "${!efvar:-}" && any_enabled=1
done
if [ "$any_enabled" -eq 0 ]; then
  echo "no models enabled (set WIAB_<ROLE>_ENABLED / WIAB_<ROLE>_MODEL_FILE); nothing to fetch"
  exit 0
fi

if ! command -v azcopy >/dev/null 2>&1; then
  echo "error: azcopy not found on PATH (macOS: 'brew install azcopy')" >&2
  exit 1
fi
if [ -z "${WIAB_MODELS_URL:-}" ]; then
  echo "error: WIAB_MODELS_URL is not set (Azure container URL with SAS token)" >&2
  exit 1
fi

mkdir -p "${MODELS_DIR}"

# Build the blob URL for one file by inserting the filename ahead of the SAS query string.
models_blob_url() {
  local file="$1" base sas
  base="${WIAB_MODELS_URL%%\?*}"
  if [ "${base}" = "${WIAB_MODELS_URL}" ]; then
    printf '%s/%s' "${base%/}" "${file}"          # no SAS query string present
  else
    sas="${WIAB_MODELS_URL#*\?}"
    printf '%s/%s?%s' "${base%/}" "${file}" "${sas}"
  fi
}

for mfvar in $(roles_with_files); do
  role="${mfvar#WIAB_}"; role="${role%_MODEL_FILE}"
  efvar="WIAB_${role}_ENABLED"
  file="${!mfvar:-}"
  is_enabled "${!efvar:-}" || continue
  if [ -z "$file" ]; then
    echo "error: $role enabled but $mfvar is empty" >&2
    exit 1
  fi
  echo "fetching ${file} -> ${MODELS_DIR}/${file}"
  azcopy copy "$(models_blob_url "${file}")" "${MODELS_DIR}/${file}" --overwrite=ifSourceNewer
done

echo "done. models in ${MODELS_DIR}"
