# Architectural Decision Records (ADRs)

This document tracks decisions made that deviate from or expand upon the original `INITIAL_SPEC.md`, detailing the rationale and consequences.

## [ADR-001] Sorting Context in Person Names

**Date:** 2026-03-30
**Context:** The GEDCOM standard and Gramps both support compound surnames with connectors (e.g. "van der Bilt", "de la Torre"). When compiling a `sort_key()` for index building and table displaying, there was an open question on whether to include the connector ("van der bilt") or sort by root surname ("bilt").
**Decision:** Connectors will be **ignored** by default when computing `sort_key()`, sorting "van der Bilt" under **"B"**, not "V". This matches linguistic library standards for genealogical indexing. The `sort_as` manual override on `PersonName` remains the ultimate escape hatch if a particular cultural exception is needed.

## [ADR-002] Primary Name Getter

**Date:** 2026-03-30
**Context:** A `Person` entity has a `Vec<PersonName>`. When needing to display the entity in a UI tree or output it to a basic export where only a single `NAME` block is allowed, querying logic was historically placed into the presentation layer.
**Decision:** Implemented a `.primary_name()` getter on the `Person` struct within the core domain layer. It will natively select the first `name_type: Birth` variant if found, falling back to the 0th index if no `Birth` name exists, and falling back to a static "Unknown" if the list is empty. This prevents duplicate fallback logic from forming in web/app clients.

## [ADR-003] GEDCOM Crate Depends on Storage Layer

**Date:** 2026-03-30
**Context:** The original CLAUDE.md stated the `gedcom` crate should have "No
direct database access". The spec intent was to keep the crate's primary job
(text parsing / domain mapping) decoupled from persistence. However,
`import_gedcom_to_sqlite` needs to write the parsed graph to SQLite inside a
single transaction for atomicity and performance.
**Decision:** Allow `rustygene-gedcom` to depend on `rustygene-storage` and
`rusqlite` directly. The parsing and semantic mapping functions remain pure
(working only on `GedcomNode` trees and core types), but the top-level import
function takes a `&mut rusqlite::Connection` and performs the write. This is a
pragmatic choice over the alternative of returning an intermediate in-memory
graph and requiring the caller to persist it — which would force every caller
to duplicate the import transaction logic. CLAUDE.md updated accordingly.
**Consequence:** The gedcom crate cannot be used without a storage dependency.
An intermediate "in-memory GEDCOM graph" layer is noted as a future
refactoring if the gedcom crate ever needs to run in a pure-parsing context.

## [ADR-004] Co-Storing Family and Relationship in One Table

**Date:** 2026-03-30
**Context:** GEDCOM distinguishes between permanent family units (`FAM`) and
looser biographical relationships (e.g., companion, partner). The storage
schema needed a place for both. Creating two separate tables (`families`,
`relationships`) seemed redundant for Phase 1A since the query patterns are
nearly identical.
**Decision:** Both `Family` and `Relationship` entities are stored in the
`families` SQLite table. They are discriminated at query time by the presence
of a `relationship_type` JSON field: `Relationship` rows always carry it,
`Family` rows never do. The `list_families` and `list_relationships` storage
methods, and the GEDCOM export helpers, use
`json_extract(data, '$.relationship_type') IS [NOT] NULL` to filter.
**Consequence:** Any caller that forgets to filter will silently mix the two
types and receive deserialization errors. This risk is mitigated by the
`list_families` / `list_relationships` implementations in `SqliteBackend`,
which apply the filter automatically. See also `docs/GEDCOM_GAPS.md`.

## [ADR-004-REMEDIATION] Separating Family and Relationship Tables

**Date:** 2026-03-31
**Context:** The original ADR-004 co-storage approach violated principle 2 of the
INITIAL_SPEC ("Family / Relationship / Event Invariants"). The single-table
design with JSON-field discrimination created implicit type contracts that were
error-prone and conflated two semantically distinct structures.
**Decision:** Split `Family` and `Relationship` into separate dedicated tables:
- `families` table: Stores only `Family` rows (grouping containers with partner refs, child links)
- `family_relationships` table: Stores `Relationship` rows (pairwise semantic edges)
The separation formalizes the architectural distinction at the schema level.
**Implementation:**
- Added migration `V003__split_families_and_relationships.sql`
- Updated storage CRUD methods (`create_relationship`, `list_relationships`, etc.) to use `family_relationships` table
- Removed `list_filtered_sync()` helper (no longer needed for JSON discrimination)
- Updated GEDCOM import to write Relationships to `family_relationships` table
- Updated e2e gate test to verify both tables are populated independently
**Consequence:** Enforcement at schema level prevents silent mixing. Simpler query logic (no WHERE clauses needed for type filtering). Aligns code implementation with domain model semantics (Principle 2). Breaking schema change requiring migration on existing databases.

## [ADR-005] DateValue::Textual as Struct Variant

**Date:** 2026-03-30
**Context:** `DateValue` uses `#[serde(tag = "type")]` internal tagging. The
`Textual` variant was originally a newtype: `Textual(String)`. serde's
internal tag implementation cannot serialize newtype variants that hold a
primitive (String, int, etc.) because there is no JSON object wrapper to
inject the `"type"` discriminator into.
**Decision:** Changed `Textual(String)` to `Textual { value: String }`.
This is a struct variant, which serializes as a JSON object
`{"type":"Textual","value":"..."}` — compatible with internal tagging.
**Consequence:** All pattern matches on `DateValue::Textual` must use
`DateValue::Textual { value }` / `DateValue::Textual { .. }` form. Callers
constructing the variant must use `DateValue::Textual { value: "...".to_string() }`.

## [ADR-006] Snapshot Assertion Query Uses CAST(value AS TEXT)

**Date:** 2026-03-30
**Context:** The `rebuild_all_snapshots()` function queries
`SELECT field, value FROM assertions` to recompute entity snapshots.
SQLite stores JSON numeric values (e.g. `1`, `0`) as INTEGER type, not TEXT.
rusqlite's `row.get::<_, String>()` returns
`"Invalid column type Integer at index: 1, name: value"` for these rows.
**Decision:** Changed the query to
`SELECT field, CAST(value AS TEXT) FROM assertions ...`. SQLite's CAST safely
coerces INTEGER/REAL values to their string representations (`"1"`, `"0.5"`)
without losing information, and TEXT/BLOB values pass through unchanged.
**Consequence:** Numeric assertion values are returned as their string
representations in the snapshot context. Since `value` is treated as an opaque
JSON token at the snapshot layer, this is correct behaviour.
