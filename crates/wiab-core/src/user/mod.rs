mod access_token;
mod external_ref;
mod ssh_key;
mod ssh_key_id;
mod token_id;
mod token_scope;
#[allow(clippy::module_inception)]
mod user;
mod user_error;
mod user_id;
mod user_kind;
mod user_numbering;
mod user_repository;
mod user_snapshot;
mod user_state;

// The credential crypto seams are product-neutral and live in `authbox-core`; re-export
// them here so existing `wiab_core::user::{TokenFactory, …}` call sites keep resolving.
pub use authbox_core::{GeneratedToken, KeyFingerprinter, TokenFactory, TokenHasher};

pub use access_token::AccessToken;
pub use external_ref::ExternalRef;
pub use ssh_key::SshKey;
pub use ssh_key_id::SshKeyId;
pub use token_id::TokenId;
pub use token_scope::TokenScope;
pub use user::User;
pub use user_error::UserError;
pub use user_id::UserId;
pub use user_kind::UserKind;
pub use user_numbering::UserNumbering;
pub use user_repository::UserRepository;
pub use user_snapshot::{SshKeySnapshot, TokenScopeSnapshot, TokenSnapshot, UserSnapshot};
pub use user_state::UserState;
