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
  provision_b64 = base64encode(file("${path.module}/scripts/provision.sh"))
  dns_csv       = join(", ", var.dns_servers)

  cloud_config = templatefile("${path.module}/templates/cloud-init.yaml.tftpl", {
    hostname           = var.hostname
    fqdn               = var.domain
    ssh_authorized_key = var.ssh_authorized_key
    provision_b64      = local.provision_b64
    domain             = var.domain
    letsencrypt_email  = var.letsencrypt_email
    announced_address  = var.announced_address
    backend_repo       = var.backend_repo
    frontend_repo      = var.frontend_repo
    fc_kernel_url      = var.fc_test_kernel_url
    fc_rootfs_url      = var.fc_test_rootfs_url
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

  cpus       = var.vcpus
  memory_max = var.memory_gb * 1024 * 1024 * 1024

  # Nested virtualization so KVM + Firecracker work inside the guest.
  exp_nested_hvm = true
  auto_poweron   = true

  cloud_config         = local.cloud_config
  cloud_network_config = local.network_config

  network {
    network_id       = data.xenorchestra_network.net.id
    expected_ip_cidr = var.wait_for_ip_cidr
  }

  disk {
    sr_id      = data.xenorchestra_sr.sr.id
    name_label = "${var.hostname}-root"
    size       = var.disk_gb * 1024 * 1024 * 1024
  }
}
