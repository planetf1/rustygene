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

## Acceptance Criteria Enforcement

- **Strict Semantic Fidelity rule (Phase 1A)**: Re-importing a GEDCOM MUST result in a 100% equivalent *Assertion graph*. Do NOT water down tests (e.g., strictly to "name comparison") to fake a passing integration.
- **Fail Over Fake**: If an implementation cannot meet the explicit sub-step criteria, you MUST open a tracking issue in `bd` mapping to the missing capability. Watering down the end-to-end or acceptance gates is strictly forbidden. Any uncovered scope must be recorded in `docs/GEDCOM_GAPS.md`.
- **Write tests from the spec, not from the implementation**: Acceptance and gate tests must be derived from the spec's definition of "done" (e.g., §8.3: "compare assertion graphs"). Never write a test that only verifies what the current code happens to produce — test what the spec *requires*.
- **No silent swallowing of GEDCOM tags**: Every standard GEDCOM 5.5.1 tag encountered in a record MUST either be mapped to a domain entity/field OR explicitly logged/counted as "unhandled standard tag". The `_ => {}` pattern on known GEDCOM tags is forbidden — it silently drops data. Use an explicit list of deferred tags that increments a counter.
- **Verify bead completion against the spec sub-step text**: Before closing a bead, re-read the corresponding sub-step description in `INITIAL_SPEC.md §16.1`. Every noun and verb in the sub-step must have corresponding code and test coverage. Example: sub-step 4.3 says "Map INDI nodes to Person + PersonName + names/gender/**events**. Extract **birth date, death date**" — closing this bead requires that BIRT and DEAT tags produce Event entities.
- **Never defer spec-mandated Phase 1A work to later phases**: If `INITIAL_SPEC.md` places a capability in Phase 1A, it must be implemented in Phase 1A. `GEDCOM_GAPS.md` documents *known limitations* and *edge cases*, not core requirements that were skipped. If a Phase 1A requirement genuinely cannot be met, open a bead and flag it — do not silently relabel it as "Phase 3+".
- **Round-trip assertion graph comparison must compare field distributions**: Simply comparing assertion counts is insufficient — the same count can result from completely different assertion compositions. Compare per-entity-type, per-field breakdowns (e.g., "Person has N name assertions, N gender assertions, N event_participation assertions").

## Development Process Rules

- **Test-first for acceptance gates**: Write the gate test skeleton from the spec *before* implementing the feature. The test should fail initially; the implementation makes it pass.
- **No phantom entity exporters**: Every entity exporter function that converts a domain type to GEDCOM nodes MUST accept all data that the importer extracts. If the importer extracts events for an entity, the exporter signature must accept events. Asymmetric import/export signatures are a structural bug.
- **Bead closure checklist**: Before closing any bead, confirm: (1) the spec sub-step deliverable exists, (2) tests cover both happy path and the edge cases mentioned in the sub-step, (3) `cargo test --workspace` passes, (4) `cargo clippy --workspace --all-targets -- -D warnings` passes.

## CI Expectations

Every pull request must pass the quality gates cleanly:
1. `cargo clippy` with zero warnings (`-D warnings`).
2. `cargo fmt --check` succeeds.
3. `cargo test` passes 100%, especially the semantic round-trip tests for GEDCOM using the files in `testdata/gedcom/`.
