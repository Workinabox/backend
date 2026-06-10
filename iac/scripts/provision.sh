#!/usr/bin/env bash
# WorkInABox host provisioning. Runs once on first boot via cloud-init.
# Config is read from /etc/wiab/provision.env (written by cloud-init).
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive

set -a
. /etc/wiab/provision.env
set +a

log() { echo "[wiab-provision] $*"; }

# ---------------------------------------------------------------------------
# 1. Packages
# ---------------------------------------------------------------------------
log "installing packages"
apt-get update -y
apt-get install -y --no-install-recommends \
  ca-certificates curl jq tar coreutils ufw \
  nginx certbot python3-certbot-nginx \
  qemu-kvm libvirt-daemon-system libvirt-clients bridge-utils cpu-checker \
  libssl3 libopus0
update-ca-certificates || true

# ---------------------------------------------------------------------------
# 2. KVM + Firecracker  (nested-virt gate)
# ---------------------------------------------------------------------------
if [ ! -e /dev/kvm ]; then
  log "FATAL: /dev/kvm absent — nested virtualization is not active on this VM"
  exit 1
fi
log "/dev/kvm present"
kvm-ok || true

ARCH="$(uname -m)"
FC_TAG="$(curl -fsSL https://api.github.com/repos/firecracker-microvm/firecracker/releases/latest | jq -r .tag_name)"
log "installing firecracker ${FC_TAG} (${ARCH})"
curl -fsSL -o /tmp/firecracker.tgz \
  "https://github.com/firecracker-microvm/firecracker/releases/download/${FC_TAG}/firecracker-${FC_TAG}-${ARCH}.tgz"
tar -xzf /tmp/firecracker.tgz -C /tmp
install -m 0755 "/tmp/release-${FC_TAG}-${ARCH}/firecracker-${FC_TAG}-${ARCH}" /usr/local/bin/firecracker
if [ -f "/tmp/release-${FC_TAG}-${ARCH}/jailer-${FC_TAG}-${ARCH}" ]; then
  install -m 0755 "/tmp/release-${FC_TAG}-${ARCH}/jailer-${FC_TAG}-${ARCH}" /usr/local/bin/jailer
fi
firecracker --version

log "firecracker microVM boot smoke test"
mkdir -p /opt/wiab/fc-test
curl -fsSL -o /opt/wiab/fc-test/vmlinux "${WIAB_FC_KERNEL_URL}"
curl -fsSL -o /opt/wiab/fc-test/rootfs.ext4 "${WIAB_FC_ROOTFS_URL}"
cat > /opt/wiab/fc-test/config.json <<'JSON'
{
  "boot-source": {
    "kernel_image_path": "/opt/wiab/fc-test/vmlinux",
    "boot_args": "console=ttyS0 reboot=k panic=1 pci=off"
  },
  "drives": [
    {
      "drive_id": "rootfs",
      "path_on_host": "/opt/wiab/fc-test/rootfs.ext4",
      "is_root_device": true,
      "is_read_only": true
    }
  ],
  "machine-config": { "vcpu_count": 1, "mem_size_mib": 128 }
}
JSON
# A booted guest kernel prints its "Linux version ..." banner over ttyS0;
# seeing it proves KVM actually executed guest code (true nested virt).
timeout 30 firecracker --no-api --config-file /opt/wiab/fc-test/config.json \
  > /opt/wiab/fc-test/console.log 2>&1 || true
if grep -qi "Linux version" /opt/wiab/fc-test/console.log; then
  log "firecracker smoke test PASSED (guest kernel booted under KVM)"
else
  log "FATAL: firecracker guest kernel did not boot — see /opt/wiab/fc-test/console.log"
  exit 1
fi

# ---------------------------------------------------------------------------
# 3. wiab system user
# ---------------------------------------------------------------------------
id wiab >/dev/null 2>&1 || useradd --system --no-create-home --shell /usr/sbin/nologin wiab

# ---------------------------------------------------------------------------
# 4. Backend — latest GitHub release
# ---------------------------------------------------------------------------
log "downloading latest backend release from ${WIAB_BACKEND_REPO}"
mkdir -p /tmp/wiab-backend
b_api="https://api.github.com/repos/${WIAB_BACKEND_REPO}/releases/latest"
b_json="$(curl -fsSL "$b_api")"
b_tgz="$(echo "$b_json" | jq -r '.assets[] | select(.name|test("x86_64-linux-gnu\\.tar\\.gz$")) | .browser_download_url')"
b_sha="$(echo "$b_json" | jq -r '.assets[] | select(.name|test("x86_64-linux-gnu\\.sha256$")) | .browser_download_url')"
[ -n "$b_tgz" ] || { log "FATAL: no backend tar.gz asset in latest release"; exit 1; }
curl -fsSL -o /tmp/wiab-backend/wiab.tar.gz "$b_tgz"
tar -xzf /tmp/wiab-backend/wiab.tar.gz -C /tmp/wiab-backend
if [ -n "$b_sha" ] && [ "$b_sha" != "null" ]; then
  curl -fsSL -o /tmp/wiab-backend/wiab.sha256 "$b_sha"
  exp="$(awk '{print $1}' /tmp/wiab-backend/wiab.sha256)"
  got="$(sha256sum /tmp/wiab-backend/wiab | awk '{print $1}')"
  [ "$exp" = "$got" ] || { log "FATAL: backend sha256 mismatch"; exit 1; }
  log "backend sha256 verified"
fi
install -m 0755 /tmp/wiab-backend/wiab /usr/local/bin/wiab
# Shared libraries built by native deps (libllama/libggml), bundled in the release
if ls /tmp/wiab-backend/lib/*.so* >/dev/null 2>&1; then
  cp -P /tmp/wiab-backend/lib/*.so* /usr/local/lib/
  ldconfig
  log "installed bundled shared libraries"
fi

# ---------------------------------------------------------------------------
# 5. Backend env + systemd service
# ---------------------------------------------------------------------------
mkdir -p /etc/wiab
cat > /etc/wiab/wiab.env <<EOF
WIAB_MEDIASOUP_LISTEN_IP=0.0.0.0
WIAB_MEDIASOUP_ANNOUNCED_ADDRESS=${WIAB_ANNOUNCED_ADDRESS}
EOF

cat > /etc/systemd/system/wiab.service <<'EOF'
[Unit]
Description=WorkInABox backend (wiab)
After=network-online.target
Wants=network-online.target

[Service]
User=wiab
EnvironmentFile=/etc/wiab/wiab.env
ExecStart=/usr/local/bin/wiab
Restart=on-failure
RestartSec=3
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable --now wiab

# ---------------------------------------------------------------------------
# 6. Frontend — latest GitHub release
# ---------------------------------------------------------------------------
log "downloading latest frontend release from ${WIAB_FRONTEND_REPO}"
mkdir -p /tmp/wiab-frontend
f_api="https://api.github.com/repos/${WIAB_FRONTEND_REPO}/releases/latest"
f_json="$(curl -fsSL "$f_api")"
f_tgz="$(echo "$f_json" | jq -r '.assets[] | select(.name|test("dist\\.tar\\.gz$")) | .browser_download_url')"
f_sha="$(echo "$f_json" | jq -r '.assets[] | select(.name|test("dist\\.tar\\.gz\\.sha256$")) | .browser_download_url')"
[ -n "$f_tgz" ] || { log "FATAL: no frontend dist asset in latest release"; exit 1; }
curl -fsSL -o /tmp/wiab-frontend/dist.tar.gz "$f_tgz"
if [ -n "$f_sha" ] && [ "$f_sha" != "null" ]; then
  curl -fsSL -o /tmp/wiab-frontend/dist.tar.gz.sha256 "$f_sha"
  exp="$(awk '{print $1}' /tmp/wiab-frontend/dist.tar.gz.sha256)"
  got="$(sha256sum /tmp/wiab-frontend/dist.tar.gz | awk '{print $1}')"
  [ "$exp" = "$got" ] || { log "FATAL: frontend sha256 mismatch"; exit 1; }
  log "frontend sha256 verified"
fi
tar -xzf /tmp/wiab-frontend/dist.tar.gz -C /tmp/wiab-frontend
rm -rf /var/www/wiab
mkdir -p /var/www/wiab
cp -r /tmp/wiab-frontend/dist/* /var/www/wiab/
chown -R www-data:www-data /var/www/wiab

# ---------------------------------------------------------------------------
# 7. nginx — serve SPA, proxy /api to backend (prefix-stripped), WS upgrade
# ---------------------------------------------------------------------------
cat > /etc/nginx/conf.d/wiab-upgrade.conf <<'EOF'
map $http_upgrade $connection_upgrade {
    default upgrade;
    ''      close;
}
EOF

cat > /etc/nginx/sites-available/wiab <<'EOF'
server {
    listen 80;
    listen [::]:80;
    server_name __DOMAIN__;

    root /var/www/wiab;
    index index.html;

    location / {
        try_files $uri $uri/ /index.html;
    }

    # Trailing slash on proxy_pass strips the /api prefix:
    #   /api/works  -> http://127.0.0.1:8080/works
    #   /api/signal -> http://127.0.0.1:8080/signal  (WebSocket)
    location /api/ {
        proxy_pass http://127.0.0.1:8080/;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection $connection_upgrade;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
EOF
sed -i "s/__DOMAIN__/${WIAB_DOMAIN}/g" /etc/nginx/sites-available/wiab
ln -sf /etc/nginx/sites-available/wiab /etc/nginx/sites-enabled/wiab
rm -f /etc/nginx/sites-enabled/default
nginx -t
systemctl reload nginx

# ---------------------------------------------------------------------------
# 8. TLS via Let's Encrypt (non-fatal: needs public DNS + port 80 reachable)
# ---------------------------------------------------------------------------
log "requesting Let's Encrypt certificate for ${WIAB_DOMAIN}"
if certbot --nginx -d "${WIAB_DOMAIN}" -m "${WIAB_LETSENCRYPT_EMAIL}" --agree-tos --redirect -n; then
  log "TLS configured"
else
  log "WARNING: certbot failed (DNS/NAT not ready yet?). Site is HTTP-only."
  log "WARNING: once DNS A record + port-80 NAT are in place, re-run:"
  log "WARNING:   certbot --nginx -d ${WIAB_DOMAIN} -m ${WIAB_LETSENCRYPT_EMAIL} --agree-tos --redirect -n"
fi

# ---------------------------------------------------------------------------
# 9. Firewall
# ---------------------------------------------------------------------------
ufw allow OpenSSH
ufw allow 'Nginx Full'
# WebRTC media. The backend uses mediasoup with an unbounded UDP port range
# (sfu.rs: port_range = None), so a tight rule isn't possible without a backend
# change. Open a pragmatic range; revisit if the backend pins a range later.
ufw allow 10000:59999/udp comment 'WebRTC media (mediasoup)'
ufw --force enable

log "provisioning complete"
