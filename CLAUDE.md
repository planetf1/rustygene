# RustyGene Development Guide & Patterns

This document outlines the core architectural boundaries, design rules, and commands to be used when developing the RustyGene project.

## Build & Test Commands

- **Build all Rust crates**: `cargo build --workspace`
- **Run all tests**: `cargo test --workspace`
- **Run lints (CI Requirement)**: `cargo clippy --workspace --all-targets -- -D warnings`
- **Format code**: `cargo fmt --all`

*Note: For the Python agents workspace (when populated), use `uv sync` and `uv run pytest`.*

## Architecture & Crate Boundaries

Dependencies must flow strictly *downward*. Circular dependencies are an architectural failure.

1. **`crates/core`**: The pure domain model (Assertions, Persons, Families, Events). **Rule:** Zero dependencies besides `serde`, `uuid`, and `chrono`. No IO, no DB code, no framework logic.
2. **`crates/storage`**: Local-first SQLite data layer. **Rule:** Depends only on `core` + database drivers (`rusqlite`/`sqlx`).
3. **`crates/gedcom`**: The GEDCOM 5.5.1 parser and emitter. **Rule:** Transforms text to `core` primitives. No direct database access.
3. **`crates/gedcom`**: The GEDCOM 5.5.1 parser and emitter. Transforms GEDCOM text to `core` primitives. The top-level `import_gedcom_to_sqlite` function depends on `storage` and `rusqlite` for transactional import. Parsing/mapping functions themselves are pure. See ADR-003 in `docs/DECISIONS.md`.
4. **`crates/api` / `crates/cli` / `app`**: Presentation and networking layers. Depend on `core` and `storage`.

## Coding Style & Rules

- **Rust Edition**: 2024 (or 2021 until fully migrated).
- **Lints**: Aim for `clippy::pedantic` compliance. Do not ignore warnings; fix them.
- **Strong Typing**: Use `enum` for variants. Avoid "stringly-typed" designs. **NO `std::any::Any` casting.**
- **Data Philosophy**: Remember the primary design principle—every genealogical fact is an `Assertion` with a confidence score and provenance, not a static boolean fact.

## CI Expectations

Every pull request must pass the quality gates cleanly:
1. `cargo clippy` with zero warnings (`-D warnings`).
2. `cargo fmt --check` succeeds.
3. `cargo test` passes 100%, especially the semantic round-trip tests for GEDCOM using the files in `testdata/gedcom/`.
