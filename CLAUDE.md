# Workinabox — Backend

## Required Reading

Before working on domain modeling or backend architecture, read:
- `../docs/DDD.md` — Domain-Driven Design reference. Defines the patterns, naming conventions, and architectural principles used in this codebase.
- `../docs/AGENT_MODEL.md` — Agent execution pipeline domain model.

## Architecture

- **wiab-core**: Domain layer. Entities, value objects, aggregates, domain services, repository traits. Minimal dependencies (thiserror, uuid, serde).
- **wiab-app**: Application layer. Use case orchestration. Depends on wiab-core.
- **wiab-inf**: Infrastructure layer. Repository implementations, external service adapters. Depends on wiab-core, wiab-app.
- **wiab** (root crate): API entry point. HTTP routes, bootstrap, config.

Dependencies point inward. The domain crate depends on nothing external. This is enforced by Cargo.toml.

## Conventions

- Rich domain models. Behavior on the types that own the data, not in separate service structs.
- Private fields + fallible constructors for value objects.
- `thiserror` for domain errors, one error enum per module.
- `#[cfg(test)] mod tests` in each module.
- Run `cargo fmt` and `cargo clippy` before considering work done.
