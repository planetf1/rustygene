# RustyGene Architecture

This document describes the actual crate structure, module boundaries, and
dependency relationships as built through Phase 1A.

For the authoritative design intent, see `docs/INITIAL_SPEC.md`.
For architectural decision records, see `docs/DECISIONS.md`.

---

## Crate Structure

```
rustygene/
├── crates/
│   ├── core/          Pure domain model (no IO)
│   ├── storage/       SQLite persistence layer
│   ├── gedcom/        GEDCOM 5.5.1 import/export
│   └── cli/           Command-line binary
└── testdata/
    └── gedcom/        Reference GEDCOM 5.5.1 files
```

### Dependency DAG

```
cli  ─────────────────────────────┐
     depends on                   │
     ├─ gedcom                    │
     │   depends on               │
     │   ├─ storage               │
     │   │   depends on           │
     │   │   └─ core              │
     │   └─ core                  │
     └─ storage                   │
         depends on               │
         └─ core                  │
                                  │
core (no project deps)  ◄─────────┘
```

Dependency direction is strictly downward. `core` has zero internal project
dependencies. All cross-cutting concerns (serde, chrono, uuid) are allowed at
every level because they are data/utility libraries, not business logic.

---

## Crate Responsibilities

### `crates/core`

The pure domain model. All genealogical entities and their associated
assertion wrappers live here.

**Modules:**
| Module        | Contents |
|---|---|
| `types`       | `EntityId` (UUID newtype), `DateValue`, `Gender`, `ActorRef`, `LineageType` |
| `person`      | `Person`, `PersonName`, `Surname`, `NameType`, `SurnameOrigin` |
| `family`      | `Family`, `Relationship`, `ChildLink`, `PartnerLink` |
| `event`       | `Event`, `EventType`, `EventParticipant`, `EventRole` |
| `place`       | `Place` |
| `evidence`    | `Repository`, `Source`, `Citation`, `CitationRef`, `Media`, `Note` |
| `assertion`   | `Assertion<T>`, `AssertionStatus`, `EvidenceType` |
| `lds`         | `LdsOrdinance`, `LdsOrdinanceType`, `LdsStatus` |
| `research`    | `ResearchLogEntry`, `SearchResult` |
| `validation`  | Domain validation rules for assertion values |

**Dependencies (external only):** `serde`, `serde_json`, `chrono`, `uuid`

**Key design invariant:** Every genealogical fact is stored as an
`Assertion<T>` with a confidence score, provenance actor, and status rather
than as a direct field on an entity. Entity structs hold a "snapshot" of
preferred/confirmed assertions for ergonomic access.

### `crates/storage`

SQLite-backed persistence implementing the `Storage` async trait.

**Public API:**
- `Storage` trait — full CRUD + pagination for all entity types
- `SqliteBackend` — concrete implementation using `Arc<Mutex<Connection>>`
- `run_migrations(conn)` — runs embedded Refinery migrations
- JSON export/import (`export_json_dump`, `import_json_dump`) — lossless
  full-database serialization for backup and transfer

**Schema overview:**
| Table            | Stores |
|---|---|
| `persons`        | `Person` JSON snapshots |
| `families`       | `Family` JSON snapshots |
| `family_relationships` | `Relationship` JSON snapshots |
| `events`         | `Event` JSON snapshots |
| `places`         | `Place` JSON snapshots |
| `sources`        | `Source` JSON snapshots |
| `citations`      | `Citation` JSON snapshots |
| `repositories`   | `Repository` JSON snapshots |
| `media`          | `Media` JSON snapshots |
| `notes`          | `Note` JSON snapshots |
| `lds_ordinances` | `LdsOrdinance` JSON snapshots |
| `assertions`     | Assertion rows with `entity_id`, `entity_type`, `field`, `value` (JSON), `confidence`, `status`, `preferred` |
| `audit_log`      | Immutable change history |
| `research_log`   | Research session entries |
| `relationships`  | Graph edges between entities |

**Migration files:** `migrations/V001__initial_schema.sql`,
`migrations/V002__add_lds_ordinances_table.sql`,
`migrations/V003__split_families_and_relationships.sql`

**Dependencies:** `core`, `rusqlite` (bundled), `refinery`, `serde_json`,
`tokio`, `chrono`, `uuid`

### `crates/gedcom`

GEDCOM 5.5.1 parser, semantic mapper, and exporter. This crate bridges GEDCOM
text files and the core domain model via the storage layer.

**Key public functions:**
| Function | Description |
|---|---|
| `import_gedcom_to_sqlite(conn, job_id, input)` | Parse GEDCOM, map to entities/assertions, persist |
| `person_to_indi_node_with_policy(person, events, places, xref, policy)` | Export a Person to a GEDCOM INDI node |
| `family_to_fam_node(family, events, places, xref)` | Export a Family to a GEDCOM FAM node |
| `source_to_sour_node(source, xref)` | Export a Source to a GEDCOM SOUR node |
| `repository_to_repo_node(repo, xref)` | Export a Repository to a GEDCOM REPO node |
| `note_to_note_node(note, xref)` | Export a Note to a GEDCOM NOTE node |
| `media_to_obje_node(media, xref)` | Export a Media record to a GEDCOM OBJE node |
| `render_gedcom_file(nodes)` | Serialize a node tree to GEDCOM 5.5.1 text |

**Import pipeline stages:**
1. Tokenize (`tokenize_gedcom`) — split GEDCOM text into `GedcomLine` items
2. Build tree (`build_gedcom_tree`) — convert flat lines into `GedcomNode` tree
3. Map persons (`map_indi_nodes_to_persons`) — INDI records → `Person` domain
   objects with embedded name slices
4. Map families (`map_family_nodes`) — FAM records → `Family` + `ChildLink`
5. Map sources/citations (`map_source_chain`) — SOUR/REPO records → evidence chain
6. Map media/notes/LDS (`map_media_note_lds`) — secondary entities
7. Generate assertions (`generate_import_assertions`) — produce `Assertion<Value>`
   rows for all extracted field values
8. Persist via `rusqlite` transaction — write all entities and assertions

**Export flow (Phase 1A — partial):**
1. Caller loads entity snapshots from SQLite (or passes them directly)
2. Per-entity `_to_xxx_node` functions build `GedcomNode` trees
3. `render_gedcom_file` serializes the tree to GEDCOM 5.5.1 text

**Known export gaps:** See `docs/GEDCOM_GAPS.md` for the current list.

**Encoding handling:** GEDCOM files are first attempted as UTF-8. If that
fails with an I/O `InvalidData` error, each byte is mapped directly to a
Unicode scalar (Latin-1 / ISO-8859-1 fallback).

**Dependencies:** `core`, `storage`, `rusqlite`, `serde_json`, `chrono`, `uuid`

### `crates/cli`

The `rustygene` command-line binary. All commands are implemented in
`crates/cli/src/main.rs` as a single-file binary.

**Commands:**

| Command | Description |
|---|---|
| `import --format gedcom <file>` | Import a GEDCOM 5.5.1 file |
| `import --format json <file-or-dir>` | Import a JSON dump |
| `export --format gedcom [--output <path>]` | Export the database as GEDCOM |
| `export --format json [--output <path-or-dir>]` | Export the database as JSON |
| `query person --name <name>` | Search persons by name |
| `show person <id>` | Show person detail with assertions |
| `show family <id>` | Show family detail |
| `show event <id>` | Show event detail |
| `research-log add ...` | Add a research log entry |
| `research-log list` | List research log entries |
| `rebuild-snapshots` | Rebuild all entity snapshots from the assertion table |

**Dependencies:** `core`, `storage`, `gedcom`, `clap`, `rusqlite`, `uuid`,
`tokio`

---

## Data Model

### Assertion-Centric Storage

Every fact about a genealogical entity is an `Assertion<Value>` with:
- `id` — UUID
- `resource_id` / `entity_id` — which entity this fact is about
- `field` — the field name (e.g., `"given_names"`, `"gender"`)
- `value` — JSON value
- `confidence` — `0.0..=1.0` float
- `status` — `pending`, `confirmed`, `rejected`
- `preferred` — boolean (only one per `entity_id + field` should be `preferred = true`)
- `proposed_by` — `ActorRef` string (user, import job, agent)
- `evidence_type` — `direct`, `indirect`, `negative`, `circumstantial`

Entity snapshot tables (`persons`, `families`, etc.) store the materialised
view of confirmed+preferred assertions for each entity, rebuilt on demand via
`rebuild_all_snapshots()`.

### Family and Relationship Storage

`Family` and `Relationship` are stored in separate tables:
- `families` stores only `Family` rows
- `family_relationships` stores only `Relationship` rows

This matches the architectural separation documented in `docs/DECISIONS.md`.

---

## Test Structure

| Test file | Tests |
|---|---|
| `crates/core/tests/property_based.rs` | Proptest: `DateValue` ordering, `Assertion` status transitions, `PersonName` serde round-trip |
| `crates/storage/tests/integration_storage.rs` | Full CRUD + audit log + research log + JSON export/import + snapshot rebuild |
| `crates/gedcom/tests/e2e_gate_test.rs` | Phase 1A gate test with per-entity/per-field assertion distribution checks for GEDCOM and JSON round-trip |
| `crates/gedcom/tests/citation_roundtrip_test.rs` | Synthetic inline citation round-trip coverage (`SOUR`/`PAGE`/`QUAY`/`DATA`/`TEXT`) |
| `crates/gedcom/tests/corpus_roundtrip_test.rs` | Phase 1B 5-vendor GEDCOM corpus import/export/re-import hardening gate |
| `crates/gedcom/tests/torture551_tag_accounting_test.rs` | Enforces zero unhandled standard GEDCOM tags for `torture551.ged` |

Running all tests: `cargo test --workspace`

---

## Remaining Phase 1A Work

No open Phase 1A blockers remain.

## Phase 2+ Roadmap

The following items are deferred beyond Phase 1A:
- Full GEDCOM tag coverage: NOTE, REPO, OBJE, ASSO, CHAN
- xref alias table for preserving original GEDCOM IDs
- Gramps XML import
- Full-text search (FTS5) with phonetic/fuzzy matching
- REST API layer
