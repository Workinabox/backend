#!/usr/bin/env bash
# Deploys the wiab backend and/or frontend from GitHub releases, in place.
# Idempotent (skips if the resolved tag is already deployed). The backend update
# health-checks after restart and auto-rolls-back to the previous build on failure.
#
# Usage: wiab-deploy --backend <version|latest|skip> --frontend <version|latest|skip> [--force]
#
# Repo names come from /etc/wiab/provision.env (WIAB_BACKEND_REPO/WIAB_FRONTEND_REPO).
# Must run as root.
set -euo pipefail

set -a
. /etc/wiab/provision.env
set +a

BACKEND_SPEC="skip"
FRONTEND_SPEC="skip"
FORCE=0
while [ $# -gt 0 ]; do
  case "$1" in
    --backend) BACKEND_SPEC="$2"; shift 2 ;;
    --frontend) FRONTEND_SPEC="$2"; shift 2 ;;
    --force) FORCE=1; shift ;;
    *) echo "wiab-deploy: unknown arg '$1'" >&2; exit 2 ;;
  esac
done

log() { echo "[wiab-deploy] $*"; }

VERSIONS_FILE=/etc/wiab/versions
RELEASES_DIR=/var/www/wiab-releases

TMPDIRS=()
cleanup() { [ ${#TMPDIRS[@]} -gt 0 ] && rm -rf "${TMPDIRS[@]}" || true; }
trap cleanup EXIT
mktmp() { local d; d="$(mktemp -d)"; TMPDIRS+=("$d"); echo "$d"; }

get_recorded() { # $1 = key
  [ -f "$VERSIONS_FILE" ] && grep -E "^$1=" "$VERSIONS_FILE" | tail -1 | cut -d= -f2 || true
}
set_recorded() { # $1 = key, $2 = value
  mkdir -p /etc/wiab; touch "$VERSIONS_FILE"
  if grep -qE "^$1=" "$VERSIONS_FILE"; then
    sed -i "s#^$1=.*#$1=$2#" "$VERSIONS_FILE"
  else
    echo "$1=$2" >> "$VERSIONS_FILE"
  fi
}

release_json() { # $1 = repo, $2 = spec (latest|vX.Y.Z)
  local url
  if [ "$2" = "latest" ]; then
    url="https://api.github.com/repos/$1/releases/latest"
  else
    url="https://api.github.com/repos/$1/releases/tags/$2"
  fi
  curl -fsSL "$url"
}

backend_healthy() {
  local i
  for i in $(seq 1 15); do
    curl -fsS -o /dev/null http://127.0.0.1:8080/health 2>/dev/null && return 0
    sleep 1
  done
  return 1
}

deploy_backend() {
  local spec="$1"
  [ "$spec" = "skip" ] && { log "backend: skip"; return 0; }

  local json tag tgz sha tmp bak exp got bn f
  json="$(release_json "$WIAB_BACKEND_REPO" "$spec")"
  tag="$(echo "$json" | jq -r '.tag_name')"
  [ -n "$tag" ] && [ "$tag" != "null" ] || { log "FATAL: backend release '$spec' not found"; exit 1; }
  if [ "$FORCE" -ne 1 ] && [ "$(get_recorded WIAB_BACKEND_VERSION)" = "$tag" ]; then
    log "backend: already $tag, skip"; return 0
  fi

  tgz="$(echo "$json" | jq -r '.assets[] | select(.name|test("x86_64-linux-gnu\\.tar\\.gz$")) | .browser_download_url')"
  sha="$(echo "$json" | jq -r '.assets[] | select(.name|test("x86_64-linux-gnu\\.sha256$")) | .browser_download_url')"
  [ -n "$tgz" ] && [ "$tgz" != "null" ] || { log "FATAL: backend $tag has no tarball asset"; exit 1; }

  tmp="$(mktmp)"
  log "backend: downloading $tag"
  curl -fsSL -o "$tmp/wiab.tar.gz" "$tgz"
  tar -xzf "$tmp/wiab.tar.gz" -C "$tmp"
  if [ -n "$sha" ] && [ "$sha" != "null" ]; then
    curl -fsSL -o "$tmp/wiab.sha256" "$sha"
    exp="$(awk '{print $1}' "$tmp/wiab.sha256")"
    got="$(sha256sum "$tmp/wiab" | awk '{print $1}')"
    [ "$exp" = "$got" ] || { log "FATAL: backend sha256 mismatch"; exit 1; }
  fi

  # Snapshot current build (binary + the lib names this release ships) for rollback.
  bak="$(mktmp)"; mkdir -p "$bak/lib"
  [ -x /usr/local/bin/wiab ] && cp -P /usr/local/bin/wiab "$bak/wiab"
  if ls "$tmp"/lib/*.so* >/dev/null 2>&1; then
    for f in "$tmp"/lib/*.so*; do
      bn="$(basename "$f")"
      [ -e "/usr/local/lib/$bn" ] && cp -P "/usr/local/lib/$bn" "$bak/lib/$bn"
    done
  fi

  install -m 0755 "$tmp/wiab" /usr/local/bin/wiab
  if ls "$tmp"/lib/*.so* >/dev/null 2>&1; then
    cp -P "$tmp"/lib/*.so* /usr/local/lib/; ldconfig
  fi

  log "backend: restarting wiab @ $tag"
  systemctl restart wiab || true
  if backend_healthy; then
    set_recorded WIAB_BACKEND_VERSION "$tag"
    log "backend: $tag healthy"
  else
    log "ERROR: backend $tag failed health check — rolling back"
    [ -e "$bak/wiab" ] && install -m 0755 "$bak/wiab" /usr/local/bin/wiab
    ls "$bak"/lib/*.so* >/dev/null 2>&1 && { cp -P "$bak"/lib/*.so* /usr/local/lib/; ldconfig; }
    systemctl restart wiab || true
    if backend_healthy; then log "backend: rolled back to previous build"; else log "backend: ROLLBACK ALSO UNHEALTHY — investigate"; fi
    exit 1
  fi
}

deploy_frontend() {
  local spec="$1"
  [ "$spec" = "skip" ] && { log "frontend: skip"; return 0; }

  local json tag tgz sha tmp exp got target tmplink
  json="$(release_json "$WIAB_FRONTEND_REPO" "$spec")"
  tag="$(echo "$json" | jq -r '.tag_name')"
  [ -n "$tag" ] && [ "$tag" != "null" ] || { log "FATAL: frontend release '$spec' not found"; exit 1; }
  if [ "$FORCE" -ne 1 ] && [ "$(get_recorded WIAB_FRONTEND_VERSION)" = "$tag" ]; then
    log "frontend: already $tag, skip"; return 0
  fi

  tgz="$(echo "$json" | jq -r '.assets[] | select(.name|test("dist\\.tar\\.gz$")) | .browser_download_url')"
  sha="$(echo "$json" | jq -r '.assets[] | select(.name|test("dist\\.tar\\.gz\\.sha256$")) | .browser_download_url')"
  [ -n "$tgz" ] && [ "$tgz" != "null" ] || { log "FATAL: frontend $tag has no dist asset"; exit 1; }

  tmp="$(mktmp)"
  log "frontend: downloading $tag"
  curl -fsSL -o "$tmp/dist.tar.gz" "$tgz"
  if [ -n "$sha" ] && [ "$sha" != "null" ]; then
    curl -fsSL -o "$tmp/dist.tar.gz.sha256" "$sha"
    exp="$(awk '{print $1}' "$tmp/dist.tar.gz.sha256")"
    got="$(sha256sum "$tmp/dist.tar.gz" | awk '{print $1}')"
    [ "$exp" = "$got" ] || { log "FATAL: frontend sha256 mismatch"; exit 1; }
  fi
  tar -xzf "$tmp/dist.tar.gz" -C "$tmp"

  mkdir -p "$RELEASES_DIR"
  target="$RELEASES_DIR/$tag"
  rm -rf "$target"; mkdir -p "$target"
  cp -r "$tmp"/dist/* "$target/"
  chown -R www-data:www-data "$target"

  # Atomically point /var/www/wiab at the new release (migrate from a plain dir if needed).
  if [ -d /var/www/wiab ] && [ ! -L /var/www/wiab ]; then rm -rf /var/www/wiab; fi
  tmplink="$(mktemp -u /var/www/.wiab-link.XXXXXX)"
  ln -s "$target" "$tmplink"
  mv -Tf "$tmplink" /var/www/wiab
  nginx -s reload 2>/dev/null || systemctl reload nginx || true
  set_recorded WIAB_FRONTEND_VERSION "$tag"
  log "frontend: $tag deployed"

  # Keep only the 3 most recent release dirs.
  ls -1dt "$RELEASES_DIR"/*/ 2>/dev/null | tail -n +4 | xargs -r rm -rf
}

deploy_backend "$BACKEND_SPEC"
deploy_frontend "$FRONTEND_SPEC"
log "deploy complete"
