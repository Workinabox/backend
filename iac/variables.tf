# ---------------------------------------------------------------------------
# Xen Orchestra connection
# ---------------------------------------------------------------------------
variable "xoa_url" {
  type        = string
  description = "Xen Orchestra websocket URL, e.g. wss://xoa.lan"
}

variable "xoa_token" {
  type        = string
  description = "Xen Orchestra API token"
  sensitive   = true
}

variable "xoa_insecure" {
  type        = string
  description = "Set to \"true\" to skip TLS verification (self-signed XOA cert)"
  default     = "false"
}

# ---------------------------------------------------------------------------
# Pool inventory (names must exist in your XO)
# ---------------------------------------------------------------------------
variable "template_name" {
  type        = string
  description = "Name of the Ubuntu 24.04 cloud-init template in XO"
}

variable "network_name" {
  type        = string
  description = "Name of the XO network to attach the VM to"
}

variable "storage_repository" {
  type        = string
  description = "Name of the storage repository (SR) for the VM disk"
}

# ---------------------------------------------------------------------------
# VM sizing
# ---------------------------------------------------------------------------
variable "hostname" {
  type        = string
  description = "VM name label and guest hostname"
  default     = "workinabox"
}

variable "vcpus" {
  type    = number
  default = 4
}

variable "memory_gb" {
  type    = number
  default = 8
}

variable "disk_gb" {
  type    = number
  default = 40
}

# ---------------------------------------------------------------------------
# Networking (static LAN IP)
# ---------------------------------------------------------------------------
variable "host_ip" {
  type        = string
  description = "Static LAN IP for the VM (outside the router DHCP pool)"
}

variable "cidr_prefix" {
  type        = number
  description = "LAN subnet prefix length, e.g. 24"
  default     = 24
}

variable "gateway" {
  type        = string
  description = "LAN default gateway"
}

variable "dns_servers" {
  type        = list(string)
  description = "DNS resolvers for the guest"
  default     = ["1.1.1.1", "9.9.9.9"]
}

variable "wait_for_ip_cidr" {
  type        = string
  description = "Terraform waits until the VM reports an IPv4 in this CIDR (guest-tools required). 0.0.0.0/0 = any."
  default     = "0.0.0.0/0"
}

variable "ssh_authorized_key" {
  type        = string
  description = "Public SSH key injected for the ubuntu user"
}

# ---------------------------------------------------------------------------
# Application config
# ---------------------------------------------------------------------------
variable "domain" {
  type        = string
  description = "FQDN served by nginx, e.g. workinabox.gos.dk"
}

variable "letsencrypt_email" {
  type        = string
  description = "Contact email for Let's Encrypt registration"
}

variable "announced_address" {
  type        = string
  description = "Address WebRTC/mediasoup announces to clients (public WAN IP if served via NAT, else host_ip)"
}

variable "backend_repo" {
  type        = string
  description = "GitHub owner/repo for the backend release"
  default     = "Workinabox/backend"
}

variable "frontend_repo" {
  type        = string
  description = "GitHub owner/repo for the frontend release"
  default     = "Workinabox/frontend"
}

# ---------------------------------------------------------------------------
# Firecracker smoke-test artifacts (bump as upstream rotates them)
# ---------------------------------------------------------------------------
variable "fc_test_kernel_url" {
  type        = string
  description = "URL to an uncompressed vmlinux for the Firecracker boot smoke test"
  default     = "https://s3.amazonaws.com/spec.ccfc.min/firecracker-ci/v1.10/x86_64/vmlinux-5.10.223"
}

variable "fc_test_rootfs_url" {
  type        = string
  description = "URL to an ext4 rootfs for the Firecracker boot smoke test"
  default     = "https://s3.amazonaws.com/spec.ccfc.min/firecracker-ci/v1.10/x86_64/ubuntu-22.04.ext4"
}
