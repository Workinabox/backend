//! Infrastructure seams for credential crypto the domain can't carry itself (random
//! generation, hashing, SSH-key fingerprinting). Ports here; impls in the infra layer.

mod ports;

pub use ports::{GeneratedToken, KeyFingerprinter, TokenFactory, TokenHasher};
