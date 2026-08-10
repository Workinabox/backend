use std::time::Instant;

use opentelemetry::KeyValue;
use wiab_app::{RuntimeHandle, VmRuntime, VmRuntimeError, VmSpec};

use crate::{DockerRuntime, FirecrackerRuntime};

/// Enum-dispatch over the concrete `VmRuntime` implementations, chosen at bootstrap. Lets the
/// vm service have one concrete runtime type (`VmApplicationService<_, _, VmRuntimeDispatch>`)
/// while the actual runtime is selected at startup (Firecracker on a KVM host, else Docker).
///
/// Also the metering point: every launch and shutdown passes through here, so
/// boot/shutdown durations, the live-VM gauge, and the launch-error counter
/// are recorded once for both runtimes.
pub enum VmRuntimeDispatch {
    Docker(DockerRuntime),
    // Boxed so this variant (which owns the whole FirecrackerConfig) isn't far larger than the
    // Docker one — clippy::large_enum_variant.
    Firecracker(Box<FirecrackerRuntime>),
}

impl VmRuntimeDispatch {
    fn runtime_label(&self) -> &'static str {
        match self {
            Self::Docker(_) => "docker",
            Self::Firecracker(_) => "firecracker",
        }
    }
}

impl VmRuntime for VmRuntimeDispatch {
    async fn launch(&self, spec: VmSpec) -> Result<RuntimeHandle, VmRuntimeError> {
        let label = self.runtime_label();
        let started = Instant::now();
        let result = match self {
            Self::Docker(runtime) => runtime.launch(spec).await,
            Self::Firecracker(runtime) => runtime.launch(spec).await,
        };
        let telemetry = wiab_telemetry::metrics();
        match &result {
            Ok(_) => {
                telemetry.vm_boot_duration.record(
                    started.elapsed().as_secs_f64(),
                    &[KeyValue::new("wiab.vm.runtime", label)],
                );
                telemetry
                    .vms_active
                    .add(1, &[KeyValue::new("wiab.vm.runtime", label)]);
            }
            Err(_) => {
                telemetry
                    .vm_launch_errors
                    .add(1, &[KeyValue::new("wiab.vm.runtime", label)]);
            }
        }
        result
    }

    async fn shutdown(&self, vm_id: &str) -> Result<(), VmRuntimeError> {
        let label = self.runtime_label();
        let started = Instant::now();
        let result = match self {
            Self::Docker(runtime) => runtime.shutdown(vm_id).await,
            Self::Firecracker(runtime) => runtime.shutdown(vm_id).await,
        };
        let telemetry = wiab_telemetry::metrics();
        telemetry.vm_shutdown_duration.record(
            started.elapsed().as_secs_f64(),
            &[KeyValue::new("wiab.vm.runtime", label)],
        );
        // Decremented whatever the outcome: the runtime tears the VM down on
        // its error paths too, and a stuck gauge is worse than an early one.
        telemetry
            .vms_active
            .add(-1, &[KeyValue::new("wiab.vm.runtime", label)]);
        result
    }
}
