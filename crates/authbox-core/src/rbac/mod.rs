//! Generic role-based access control: a role ladder, an action→role mapping, opaque
//! resource references, and an `effective_role` policy parameterised by a product-supplied
//! [`ResourceHierarchy`].

mod grant;
mod operation;
mod policy;
mod resource_hierarchy;
mod resource_ref;
mod role;
mod role_error;

pub use grant::Grant;
pub use operation::Operation;
pub use policy::effective_role;
pub use resource_hierarchy::ResourceHierarchy;
pub use resource_ref::ResourceRef;
pub use role::Role;
pub use role_error::RoleError;
