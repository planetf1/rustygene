# Agent Instructions

This file contains rules and conventions for **all** AI coding agents working on this project (GitHub Copilot, Claude, Codex, and others). Read this file in full before making changes.

For detailed architecture and design intent, see `docs/INITIAL_SPEC.md`.  
For architectural decisions and deviations, see `docs/DECISIONS.md`.  
For the current build state, see `docs/ARCHITECTURE.md`.  
For known GEDCOM limitations, see `docs/GEDCOM_GAPS.md`.

---

## Build & Test Commands

```bash
cargo build --workspace                                  # Build all crates
cargo test --workspace                                   # Run all tests
cargo clippy --workspace --all-targets -- -D warnings     # Lint (CI gate)
cargo fmt --all                                          # Format code
cargo fmt --all -- --check                               # Format check (CI gate)
```

*Python agents workspace (when populated):* `uv sync && uv run pytest`

---

## Architecture & Crate Boundaries

Dependencies flow strictly **downward**. Circular dependencies are an architectural failure.

1. **`crates/core`** — Pure domain model (Assertions, Persons, Families, Events). **Zero** dependencies beyond `serde`, `uuid`, `chrono`. No IO, no DB, no framework logic.
2. **`crates/storage`** — SQLite persistence. Depends only on `core` + `rusqlite`.
3. **`crates/gedcom`** — GEDCOM 5.5.1 parser and emitter. Pure parsing/mapping functions + a top-level `import_gedcom_to_sqlite` that depends on `storage`/`rusqlite` for transactional import (see ADR-003 in `docs/DECISIONS.md`).
4. **`crates/cli`** — CLI binary. Depends on `core`, `storage`, `gedcom`.
5. **`crates/api`** / **`app`** — (Future) presentation and networking layers.

---

## Coding Style & Rules

- **Rust Edition**: 2024 (or 2021 until fully migrated).
- **Lints**: Aim for `clippy::pedantic` compliance. Fix warnings — do not suppress them.
- **Strong Typing**: Use `enum` for variants. No "stringly-typed" designs. No `std::any::Any` casting.
- **Data Philosophy**: Every genealogical fact is an `Assertion` with a confidence score and provenance — not a static boolean fact.

---

## Acceptance Criteria Enforcement

- **Strict Semantic Fidelity (Phase 1A)**: Re-importing a GEDCOM MUST produce a 100% equivalent *assertion graph*. Do NOT water down tests (e.g., comparing only names) to mask incomplete functionality.
- **Fail Over Fake**: If an implementation cannot meet a spec sub-step, open a tracking issue in `bd`. Watering down acceptance gates is forbidden. Document gaps in `docs/GEDCOM_GAPS.md`.
- **Tests derive from the spec, not the implementation**: Gate tests must be written from the spec's definition of "done" (e.g., §8.3: "compare assertion graphs"). Never write tests that only verify what current code produces — test what the spec *requires*.
- **No silent swallowing of GEDCOM tags**: Every standard GEDCOM 5.5.1 tag MUST either map to a domain entity/field OR be explicitly logged/counted as "unhandled standard tag". The `_ => {}` catch-all on known tags is forbidden — it silently drops data.
- **Verify bead completion against the spec**: Before closing a bead, re-read the corresponding sub-step in `INITIAL_SPEC.md §16.1`. Every noun and verb must have code and test coverage. Example: sub-step 4.3 says "Map INDI nodes ... names/gender/**events**. Extract **birth date, death date**" — BIRT and DEAT tags must produce Event entities.
- **Never defer spec-mandated Phase 1A work**: If `INITIAL_SPEC.md` places a capability in Phase 1A, implement it in Phase 1A. `GEDCOM_GAPS.md` documents *edge-case limitations*, not skipped core requirements.
- **Assertion graph comparison must compare field distributions**: Matching total counts is insufficient. Compare per-entity-type, per-field breakdowns (e.g., "Person: N name, N gender, N event_participation assertions").

---

## Development Process Rules

- **Test-first for acceptance gates**: Write the gate test skeleton from the spec *before* implementing the feature. It should fail initially; the implementation makes it pass.
- **No phantom exporters**: Every exporter that converts a domain type to GEDCOM MUST accept all data the importer extracts. Asymmetric import/export signatures are structural bugs.
- **Bead closure checklist**: Before closing any bead: (1) spec sub-step deliverable exists, (2) tests cover happy path + sub-step edge cases, (3) `cargo test --workspace` passes, (4) `cargo clippy --workspace --all-targets -- -D warnings` passes.
- **No silent `try/except` or `_ => {}`**: Fail fast and loud. Expose root causes.

---

## CI Expectations

Every pull request must pass cleanly:
1. `cargo clippy` with zero warnings (`-D warnings`).
2. `cargo fmt --check` succeeds.
3. `cargo test` passes 100%, especially GEDCOM semantic round-trip tests against `testdata/gedcom/`.

---

## Issue Tracking (Beads)

This project uses **bd** (beads) for issue tracking. Run `bd onboard` to get started.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work atomically
bd close <id>         # Complete work
bd dolt push          # Push beads data to remote
```

---

## Non-Interactive Shell Commands

**ALWAYS use non-interactive flags** with file operations to avoid hanging on confirmation prompts.

Shell commands like `cp`, `mv`, and `rm` may be aliased to include `-i` (interactive) mode on some systems, causing the agent to hang indefinitely waiting for y/n input.

**Use these forms instead:**
```bash
# Force overwrite without prompting
cp -f source dest           # NOT: cp source dest
mv -f source dest           # NOT: mv source dest
rm -f file                  # NOT: rm file

# For recursive operations
rm -rf directory            # NOT: rm -r directory
cp -rf source dest          # NOT: cp -r source dest
```

**Other commands that may prompt:**
- `scp` - use `-o BatchMode=yes` for non-interactive
- `ssh` - use `-o BatchMode=yes` to fail instead of prompting
- `apt-get` - use `-y` flag
- `brew` - use `HOMEBREW_NO_AUTO_UPDATE=1` env var

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
### Beads Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
- NEVER water down or simplify Acceptance Tests (e.g., testing only names instead of full assertion graphs) to mask incomplete functionality or force a component to pass CI.
- IF a feature gap is identified (e.g., missing GEDCOM tags, unexported models), document it in `docs/GEDCOM_GAPS.md` or open a new tracking issue with `bd` before closing your current working bead.
<!-- END BEADS INTEGRATION -->
