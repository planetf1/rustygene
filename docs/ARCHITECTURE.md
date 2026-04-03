# RustyGene Architecture

This document describes the actual crate structure, module boundaries, and
dependency relationships as built through Phase 1A and Phase 2.

For the authoritative design intent, see `docs/INITIAL_SPEC.md`.
For architectural decision records, see `docs/DECISIONS.md`.
For historical Phase 1A review, see `docs/PHASE_1A_REVIEW.md`.
For historical Phase 2 review, see `docs/PHASE_2_REVIEW.md`.

---

## Crate Structure

```
rustygene/
├── crates/
│   ├── core/          Pure domain model (no IO)
│   ├── storage/       SQLite persistence layer
│   ├── gedcom/        GEDCOM 5.5.1 import/export
│   ├── api/           Axum REST API + OpenAPI spec (utoipa)
│   └── cli/           Command-line binary
├── app/
│   ├── src/           Svelte 5 frontend
│   └── src-tauri/     Tauri desktop shell (embedded Axum server)
├── spec/
│   └── openapi.json   Generated OpenAPI specification
└── testdata/
    └── gedcom/        Reference GEDCOM 5.5.1 files
```

### Dependency DAG

```
app/src-tauri ─────────────────────┐
     depends on                    │
     ├─ api                        │
     │   depends on                │
     │   ├─ storage                │
     │   │   depends on            │
     │   │   └─ core               │
     │   └─ core                   │
     └─ storage                    │
                                   │
cli  ──────────────────────────────┤
     depends on                    │
     ├─ gedcom                     │
     │   depends on                │
     │   ├─ storage                │
     │   │   depends on            │
     │   │   └─ core               │
     │   └─ core                   │
     └─ storage                    │
                                   │
core (no project deps)  ◄──────────┘
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
| `show source <id>` | Show source detail |
| `show citation <id>` | Show citation detail |
| `show repository <id>` | Show repository detail |
| `show note <id>` | Show note detail |
| `show media <id>` | Show media detail |
| `research-log add ...` | Add a research log entry |
| `research-log list` | List research log entries |
| `sandbox create ...` | Create a research sandbox |
| `sandbox list` | List sandboxes |
| `sandbox compare ...` | Compare sandbox vs trunk |
| `staging list` | List staging queue proposals |
| `staging accept <id>` | Accept a staging proposal |
| `staging reject <id>` | Reject a staging proposal |
| `rebuild-snapshots` | Rebuild all entity snapshots from the assertion table |

**Dependencies:** `core`, `storage`, `gedcom`, `clap`, `rusqlite`, `uuid`,
`tokio`

### `crates/api`

Axum REST API with OpenAPI spec auto-generation via `utoipa`.

**Route modules:**
| Module | Endpoints |
|---|---|
| `persons` | Full CRUD, assertions, timeline, families |
| `families` | Full CRUD, assertions |
| `events` | Full CRUD, assertions, participant management |
| `sources` | Full CRUD |
| `citations` | Full CRUD |
| `repositories` | Full CRUD |
| `media` | Full CRUD, albums, tags, OCR trigger, upload |
| `notes` | Full CRUD |
| `staging` | Proposal CRUD, approve/reject, bulk operations |
| `research_log` | CRUD, filtering by person/result/date |
| `search` | Multi-strategy (exact/FTS/phonetic/combined) |
| `graph` | Ancestors, descendants, pedigree, path, network |
| `import_export` | Multipart import, export (GEDCOM/JSON/Bundle) |
| `events_sse` | Server-sent events for real-time updates |
| `assertions` | CRUD for assertions |
| `debug` | Health, metrics, logs, diagnostics bundle |

**Infrastructure:**
- OpenAPI spec at `GET /api/v1/openapi.json` (committed to `spec/openapi.json`)
- CORS configured for Tauri origins
- Request body size limits
- Event bus (broadcast channel) for real-time updates
- Domain event types: `EntityCreated`, `EntityUpdated`, etc.

**Dependencies:** `core`, `storage`, `axum`, `utoipa`, `tower-http`,
`serde_json`, `tokio`

### `app/src-tauri`

Tauri 2.x desktop shell. Starts the embedded Axum API server on app launch
and communicates the API port to the Svelte frontend.

**Tauri commands:**
- `get_api_port` — returns the running API server port
- `open_file_dialog` / `save_file_dialog` — native file dialogs
- `read_binary_file` / `write_binary_file` — filesystem operations
- `create_database_backup` / `restore_database_backup` — backup/restore

### `app/src` (Svelte 5 Frontend)

SvelteKit application with runes-based state management.

**Key routes:** persons, families, events, sources, repositories, media,
import, export, search, charts (pedigree/fan/graph), staging, research-log,
debug.

**Components:** PersonForm, FamilyForm, EventForm, CitationDetail,
CitationPicker, AssertionList, NoteList, Sidebar, Toolbar.

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

## Phase Status

### Phase 1A (Core + GEDCOM + CLI) — COMPLETE

All spec sub-steps (§16.1) delivered. Gate test passes with full
assertion-graph comparison. 212 Rust tests + 8 frontend tests passing.
No open blockers.

### Phase 1B (GEDCOM Hardening) — PARTIALLY COMPLETE

Corpus testing against 5 vendor GEDCOM files is in place. Remaining items:
- NOTE records as typed entities (currently via `_raw_gedcom`)
- Inline OBJE MediaRef linking
- ASSO association records
- Full-text search with phonetic/fuzzy matching (infrastructure exists but
  not query-exposed in Phase 1A CLI)
- Gramps XML import
- GEDCOM merge import (deterministic matching engine)

### Phase 2 (Desktop App + REST API) — COMPLETE

API backend with 16 route modules, OpenAPI spec, Tauri desktop shell,
Svelte 5 frontend with import/export wizard, entity list/detail views,
search, staging queue, and chart route skeletons. See `docs/PHASE_2_REVIEW.md`
for delivery evidence.

### Phase 3 (Sandboxes + Event Bus + Agent Infrastructure) — NOT STARTED

Per `INITIAL_SPEC.md §16`:
- Research sandbox UI (create/switch/compare/promote/discard)
- Event bus (internal channels + SSE/polling) — SSE infrastructure exists
- Staging queue review dashboard — basic implementation exists
- Agent registry and management
- Sandbox comparison + validator scoring
- Negative evidence prompt workflow

### Future Phases

- **Phase 4:** FamilySearch/Discovery connectors, document processor agent, validator agent
- **Phase 5:** Multi-user collaboration, PostgreSQL + S3, DNA integration
