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
  mem_bytes     = var.memory_gb * 1024 * 1024 * 1024

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
    ignore_changes = [power_state]
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
