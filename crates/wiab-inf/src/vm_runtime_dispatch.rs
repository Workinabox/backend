use wiab_app::{RuntimeHandle, VmRuntime, VmRuntimeError, VmSpec};

use crate::{DockerRuntime, FirecrackerRuntime};

/// Enum-dispatch over the concrete `VmRuntime` implementations, chosen at bootstrap. Lets the
/// vm service have one concrete runtime type (`VmApplicationService<_, _, VmRuntimeDispatch>`)
/// while the actual runtime is selected at startup (Firecracker on a KVM host, else Docker).
pub enum VmRuntimeDispatch {
    Docker(DockerRuntime),
    // Boxed so this variant (which owns the whole FirecrackerConfig) isn't far larger than the
    // Docker one — clippy::large_enum_variant.
    Firecracker(Box<FirecrackerRuntime>),
}

impl VmRuntime for VmRuntimeDispatch {
    async fn launch(&self, spec: VmSpec) -> Result<RuntimeHandle, VmRuntimeError> {
        match self {
            Self::Docker(runtime) => runtime.launch(spec).await,
            Self::Firecracker(runtime) => runtime.launch(spec).await,
        }
    }

    async fn shutdown(&self, vm_id: &str) -> Result<(), VmRuntimeError> {
        match self {
            Self::Docker(runtime) => runtime.shutdown(vm_id).await,
            Self::Firecracker(runtime) => runtime.shutdown(vm_id).await,
        }
    }
}
