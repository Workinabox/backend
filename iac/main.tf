data "xenorchestra_template" "ubuntu" {
  name_label = var.template_name
}

data "xenorchestra_network" "net" {
  name_label = var.network_name
}

data "xenorchestra_sr" "sr" {
  name_label = var.storage_repository
}

locals {
  provision_b64   = base64encode(file("${path.module}/scripts/provision.sh"))
  wiab_deploy_b64 = base64encode(file("${path.module}/scripts/wiab-deploy.sh"))
  dns_csv         = join(", ", var.dns_servers)
  mem_bytes       = var.memory_gb * 1024 * 1024 * 1024

  cloud_config = templatefile("${path.module}/templates/cloud-init.yaml.tftpl", {
    hostname           = var.hostname
    fqdn               = var.domain
    ssh_authorized_key = var.ssh_authorized_key
    provision_b64      = local.provision_b64
    wiab_deploy_b64    = local.wiab_deploy_b64
    domain             = var.domain
    letsencrypt_email  = var.letsencrypt_email
    announced_address  = var.announced_address
    backend_repo       = var.backend_repo
    frontend_repo      = var.frontend_repo
    backend_version    = var.backend_version
    frontend_version   = var.frontend_version
    fc_kernel_url      = var.fc_test_kernel_url
    fc_rootfs_url      = var.fc_test_rootfs_url
    db_password        = var.db_password
  })

  network_config = templatefile("${path.module}/templates/network-config.yaml.tftpl", {
    host_ip     = var.host_ip
    cidr_prefix = var.cidr_prefix
    gateway     = var.gateway
    dns_csv     = local.dns_csv
  })
}

resource "xenorchestra_vm" "host" {
  name_label       = var.hostname
  name_description = "WorkInABox host (managed by Terraform)"
  template         = data.xenorchestra_template.ubuntu.id

  cpus = var.vcpus
  # Static memory pin (min == max): nested virt rejects ballooning (MAXPIN).
  memory_max = local.mem_bytes
  memory_min = local.mem_bytes

  # Nested virt on XCP-ng 8.3 is platform:nested-virt, which this provider does
  # NOT expose (exp_nested_hvm sets the legacy, ignored key). It is enabled
  # out-of-band by null_resource.enable_nested_virt below — which must run
  # BEFORE first boot so cloud-init's KVM gate passes. So create the VM Halted;
  # that resource flips nestedVirt and starts it. ignore_changes on power_state
  # keeps later applies from fighting the externally-started VM.
  power_state  = "Halted"
  auto_poweron = true

  cloud_config         = local.cloud_config
  cloud_network_config = local.network_config

  network {
    network_id = data.xenorchestra_network.net.id
  }

  disk {
    sr_id      = data.xenorchestra_sr.sr.id
    name_label = "${var.hostname}-root"
    size       = var.disk_gb * 1024 * 1024 * 1024
  }

  lifecycle {
    # power_state: the VM is started out-of-band by enable_nested_virt.
    # cloud_config/cloud_network_config: cloud-init only runs at first boot, so editing
    # provision.sh / the templates must not force-replace the live VM. Existing VMs are
    # updated over SSH (provision_db, deploy_app); fresh VMs get the new cloud-init.
    ignore_changes = [power_state, cloud_config, cloud_network_config]
  }
}

# Enables real nested virtualization (XCP-ng 8.3 platform:nested-virt) and starts
# the VM. Requires xo-cli installed and registered on the machine running
# Terraform (same XO the provider targets). Re-pins memory defensively.
resource "null_resource" "enable_nested_virt" {
  triggers = {
    vm_id = xenorchestra_vm.host.id
    mem   = local.mem_bytes
  }

  provisioner "local-exec" {
    command = "xo-cli vm.set id=${self.triggers.vm_id} memoryMin=${self.triggers.mem} memoryMax=${self.triggers.mem} memoryStaticMax=${self.triggers.mem} nestedVirt=true && xo-cli vm.start id=${self.triggers.vm_id}"
  }
}

# Installs and configures a local PostgreSQL on the existing VM over SSH, and points the
# backend at it (WIAB_PERSISTENCE=postgres + DATABASE_URL in /etc/wiab/wiab.env). Idempotent
# and re-runnable without recreating the VM: bump db_provision_version to re-run. Runs
# before deploy_app so a newly deployed binary boots with the database already present.
resource "null_resource" "provision_db" {
  depends_on = [null_resource.enable_nested_virt]

  triggers = {
    db_provision_version = var.db_provision_version
    db_password          = var.db_password
  }

  connection {
    host        = var.host_ip
    user        = "ubuntu"
    private_key = file(pathexpand(var.ssh_private_key_path))
    timeout     = "10m"
  }

  provisioner "remote-exec" {
    inline = [
      # terraform remote-exec runs this under /bin/sh (dash), which has no `pipefail`
      # (`set -o pipefail` aborts dash with exit 2). `set -eu` is enough and portable.
      "set -eu",
      "cloud-init status --wait || true",
      # Wait up to 5 min for the apt/dpkg lock (unattended-upgrades runs at boot and
      # periodically; without this, apt aborts with exit 2 when the lock is held).
      "sudo apt-get -o DPkg::Lock::Timeout=300 update -y",
      "sudo DEBIAN_FRONTEND=noninteractive apt-get -o DPkg::Lock::Timeout=300 install -y postgresql",
      "sudo systemctl enable --now postgresql",
      # Role + database (idempotent). `|| true` swallows the harmless 'already exists'
      # error on re-runs; avoiding a pipe sidesteps the set -o pipefail / SIGPIPE trap.
      # ALTER ROLE always (re)sets the password so it matches config (errors surface).
      "sudo -u postgres psql -c \"CREATE ROLE wiab LOGIN\" || true",
      "sudo -u postgres psql -c \"ALTER ROLE wiab LOGIN PASSWORD '${var.db_password}'\"",
      "sudo -u postgres createdb -O wiab wiab || true",
      # Point the backend at Postgres (merge into wiab.env without clobbering other vars).
      "sudo sed -i '/^WIAB_PERSISTENCE=/d;/^DATABASE_URL=/d' /etc/wiab/wiab.env",
      "echo 'WIAB_PERSISTENCE=postgres' | sudo tee -a /etc/wiab/wiab.env >/dev/null",
      "echo 'DATABASE_URL=postgres://wiab:${var.db_password}@localhost:5432/wiab' | sudo tee -a /etc/wiab/wiab.env >/dev/null",
      "sudo systemctl restart wiab || true",
    ]
  }
}

# Pushes the latest wiab-deploy script and points the nginx /api proxy at the HTTPS
# backend on the EXISTING VM (cloud-init only writes these at first boot). Idempotent;
# re-runs when wiab-deploy.sh changes. Runs before deploy_app so the new health check
# (https) and proxy are in place before the binary is (re)deployed.
resource "null_resource" "reconfigure_proxy" {
  depends_on = [null_resource.enable_nested_virt]

  triggers = {
    wiab_deploy_sha = filesha256("${path.module}/scripts/wiab-deploy.sh")
    proxy_scheme    = "https"
  }

  connection {
    host        = var.host_ip
    user        = "ubuntu"
    private_key = file(pathexpand(var.ssh_private_key_path))
    timeout     = "10m"
  }

  provisioner "file" {
    source      = "${path.module}/scripts/wiab-deploy.sh"
    destination = "/tmp/wiab-deploy"
  }

  provisioner "remote-exec" {
    inline = [
      "set -eu",
      "cloud-init status --wait || true",
      "sudo install -m 0755 /tmp/wiab-deploy /usr/local/bin/wiab-deploy",
      # Point the nginx /api proxy at the HTTPS backend, with verification off for the
      # self-signed localhost hop. Both seds are idempotent (no-op once already https).
      "sudo sed -i 's#proxy_pass http://127.0.0.1:8080/;#proxy_pass https://127.0.0.1:8080/;#g' /etc/nginx/sites-available/wiab",
      "grep -q 'proxy_ssl_verify off' /etc/nginx/sites-available/wiab || sudo sed -i 's#proxy_pass https://127.0.0.1:8080/;#proxy_pass https://127.0.0.1:8080/;\\n        proxy_ssl_verify off;#g' /etc/nginx/sites-available/wiab",
      "sudo nginx -t",
      "sudo systemctl reload nginx",
    ]
  }
}

# Deploys the pinned backend/frontend versions over SSH. Re-runs whenever a
# version variable changes (bump backend_version / frontend_version and apply)
# WITHOUT recreating the VM. On first apply it waits for cloud-init to finish,
# then no-ops because wiab-deploy is idempotent (the version is already deployed).
resource "null_resource" "deploy_app" {
  depends_on = [
    null_resource.enable_nested_virt,
    null_resource.provision_db,
    null_resource.reconfigure_proxy,
  ]

  triggers = {
    backend_version  = var.backend_version
    frontend_version = var.frontend_version
  }

  connection {
    host        = var.host_ip
    user        = "ubuntu"
    private_key = file(pathexpand(var.ssh_private_key_path))
    timeout     = "10m"
  }

  provisioner "remote-exec" {
    inline = [
      "cloud-init status --wait || true",
      "sudo wiab-deploy --backend ${var.backend_version} --frontend ${var.frontend_version}",
    ]
  }
}
