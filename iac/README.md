# iac — WorkInABox host on xcp-ng

Terraform that provisions a single Ubuntu 24.04 VM on an xcp-ng pool (via Xen
Orchestra) running the full WorkInABox stack:

- **KVM + Firecracker** inside the guest (nested virtualization), with a boot
  smoke test that fails provisioning if `/dev/kvm` or a real microVM boot is not
  available.
- **Backend** (`wiab`) installed from its latest GitHub Release, run as a systemd
  service on `:8080`.
- **Frontend** installed from its latest GitHub Release, served by **nginx** over
  HTTPS (Let's Encrypt), with `/api` proxied to the backend.

All in-guest setup is done by cloud-init running [`scripts/provision.sh`](scripts/provision.sh).

## Files

| File | Purpose |
|---|---|
| `versions.tf` | Terraform + `vatesfr/xenorchestra` provider pin |
| `providers.tf` | XO connection (url/token/insecure) |
| `variables.tf` | All inputs |
| `main.tf` | Template/network/SR data sources + the `xenorchestra_vm` |
| `outputs.tf` | `host_ip`, `url` |
| `terraform.tfvars.example` | Sample inputs — copy to `terraform.tfvars` |
| `templates/cloud-init.yaml.tftpl` | cloud-config (writes env + provision.sh) |
| `templates/network-config.yaml.tftpl` | Static-IP netplan config |
| `scripts/provision.sh` | The actual in-guest setup |

## Usage

```sh
cp terraform.tfvars.example terraform.tfvars
# edit terraform.tfvars
terraform init
terraform plan
terraform apply
```

Watch in-guest progress: `ssh ubuntu@<host_ip>` then
`tail -f /var/log/wiab-provision.log` (or `/var/log/cloud-init-output.log`).

## Notes / caveats

- **Ubuntu 24.04 template required.** The backend release is built on Debian
  bookworm (glibc 2.36); 22.04 (glibc 2.35) may fail to run it.
- **Nested virt is experimental on xcp-ng** (domain crashes / reboots reported;
  Intel hosts fare better than AMD). `exp_nested_hvm = true` enables it; the
  Firecracker smoke test verifies it actually works.
- **certbot is non-fatal.** If DNS/NAT aren't ready at apply time the host comes
  up HTTP-only; re-run the `certbot --nginx ...` line from the provision log once
  ready.
- **WebRTC media (mediasoup)** is UDP straight to the host and uses an unbounded
  port range in the current backend, so the firewall opens `10000-59999/udp` as a
  pragmatic compromise. WebRTC does **not** go through nginx.
- The smoke-test kernel/rootfs URLs (`fc_test_*_url`) point at Firecracker CI
  artifacts and may need bumping over time.
