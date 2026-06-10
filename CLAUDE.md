# Workinabox

- Dont ever guess anything. Check facts ALWAYS.
- Think before coding.
- Don't assume.
- Don't hide confusion.
- Don't be afraid to ask questions.
- Don't be afraid to say "I don't know".
- Don't be afraid to say "I don't understand".
- Surface tradeoffs.
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations are possible, state them and ask which one is correct. Don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.
- Simplicity first.
- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.
- Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.
- Touch only what you must. Clean up only your own mess.
- When editing existing code:
    - Don't "improve" adjacent code, comments, or formatting.
    - Don't refactor things that aren't broken.
    - Match existing style, even if you'd do it differently.
    - If you notice unrelated dead code, mention it - don't delete it.
- When your changes create orphans:
    - Remove imports/variables/functions that YOUR changes made unused.
    - Don't remove pre-existing dead code unless asked.
- The test: Every changed line should trace directly to the user's request.
- Define success criteria. Loop until verified.
- Transform tasks into verifiable goals:
    - "Add validation" → "Write tests for invalid inputs, then make them pass"
    - "Fix the bug" → "Write a test that reproduces it, then make it pass"
    - "Refactor X" → "Ensure tests pass before and after"
- For multi-step tasks, state a brief plan:
    1. [Step] → verify: [check]
    2. [Step] → verify: [check]
    3. [Step] → verify: [check]
- Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification. 
- These guidelines are working if: fewer unnecessary changes in diffs, fewer rewrites due to overcomplication, and clarifying questions come before implementation rather than after mistakes.
- Favor smaller commits. Single responsability that concentrate on one concept/part/story/feature/struct
- Single responsability principle always

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
