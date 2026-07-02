#[allow(clippy::module_inception)]
mod vm;
mod vm_error;
mod vm_id;
mod vm_numbering;
mod vm_repository;
mod vm_resources;
mod vm_snapshot;
mod vm_state;
mod vm_template;

pub use vm::Vm;
pub use vm_error::VmError;
pub use vm_id::VmId;
pub use vm_numbering::VmNumbering;
pub use vm_repository::VmRepository;
pub use vm_resources::VmResources;
pub use vm_snapshot::VmSnapshot;
pub use vm_state::VmState;
pub use vm_template::VmTemplate;
