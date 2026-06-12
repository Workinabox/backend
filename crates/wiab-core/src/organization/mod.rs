#[allow(clippy::module_inception)]
mod organization;
mod organization_error;
mod organization_id;
mod organization_numbering;
mod organization_repository;
mod organization_snapshot;

pub use organization::Organization;
pub use organization_error::OrganizationError;
pub use organization_id::OrganizationId;
pub use organization_numbering::OrganizationNumbering;
pub use organization_repository::OrganizationRepository;
pub use organization_snapshot::OrganizationSnapshot;
