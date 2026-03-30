### Technical Specification: RustyGene — AI-Assisted Genealogy Engine

**Document Revision Date:** 2026-03-30
**Revision:** 12 (rev 8: canonicality principles, entity invariants, Phase 1A scoping, GEDCOM fidelity tiers; rev 9: snapshot consistency, relationship direction, idempotency key, Place simplification, migration tooling, audit log diffs; rev 10: competitive advantages, citation propagation, media gallery, DNA-as-source; rev 11: embedded Axum server architecture, security model, language-agnostic agent contract, sidecar packaging, web deployment portability, document lifecycle note; rev 12: GEDCOM merge import — diff/selective import workflow, entity matching engine, staging queue integration)

---

#### What Is RustyGene?

RustyGene is a local-first desktop application for serious genealogical research. It combines a Rust-based data engine with optional AI agents that can suggest discoveries, extract handwritten records, and validate research — but the human always decides what goes into the tree.

**Who is it for?** Genealogists who have outgrown Ancestry's web UI and want the rigour of Gramps with a modern interface, probabilistic reasoning about evidence, and AI assistance that respects the Genealogical Proof Standard.

**What makes it different?**

* **Assertions, not facts.** Every piece of data is a probabilistic assertion with a confidence score, evidence classification (direct/indirect/negative per Mills' taxonomy), and full source citation. Multiple competing assertions can coexist for the same field. The human resolves conflicts.
* **Research sandboxes.** Lightweight hypothesis branches — "What if this Thomas Jones belongs to a different family?" — without cloning the entire tree. Compare, score, promote, or discard.
* **AI as advisor, not authority.** Agents submit proposals to a staging queue. They never write directly to the graph. The core validates, the human reviews.
* **Domain-aware.** Handles patronymic naming systems (Welsh *ap/ab*, Scandinavian *-son/-dóttir*), imprecise dates, temporal place names, pedigree collapse, LDS ordinances, and the full Repository → Source → Citation hierarchy.
* **Lossless data.** GEDCOM import preserves everything — unknown tags stored verbatim. No silent data loss.

**Technology:**

* **Core + storage:** Rust, SQLite (single-file, ACID, local-first)
* **Desktop app:** Tauri 2.x + Svelte 5 + Cytoscape.js (relationship graphs) + D3.js (charts)
* **AI agents:** Python (out-of-process, optional, communicates via JSON/IPC)
* **Import/export:** GEDCOM 5.5.1, Gramps XML, JSON, CSV

**Current status:** Pre-implementation. This document is the complete technical specification.

**Document lifecycle:** This spec defines *intent* — the target architecture and design rationale. It should stabilise once implementation begins and change only for genuine design pivots, not implementation details. As development progresses, separate documents capture the current state:
* `docs/ARCHITECTURE.md` — actual crate structure, module boundaries, and dependency graph as built.
* `docs/DECISIONS.md` — architectural decision records (ADRs) for deviations from this spec, with rationale.
* `CLAUDE.md` — project conventions, build commands, and patterns for development sessions.
* `CHANGELOG.md` — what shipped per release.
* Beads issues — fine-grained task tracking derived from the Phase 1A sequence (§16.1).

If implementation reveals that a spec decision was wrong, record the deviation in `DECISIONS.md` rather than silently editing this spec. The spec remains the reference for original intent.

#### How RustyGene Addresses Common Genealogy Software Frustrations

These are recurring pain points from genealogy communities (Reddit, GitHub, genealogy forums) mapped to specific design decisions in this spec.

| User frustration | Affected tools | RustyGene's structural answer |
|---|---|---|
| **"The citation slog ruins my love for genealogy"** — entering the same source citation 6 times for a single census page | Ancestry, Gramps, RootsMagic | Events are shared objects with typed participants. One census Event, one Citation, automatic propagation to all participant assertions in one transaction (§3.1). |
| **"My media is an unsearchable mess"** — images sorted by upload order or hash filenames, no logical grouping | RootsMagic, Ancestry, FTM | Content-addressed storage for dedup + virtual albums, custom tags, structured captions, and sort/filter by date, entity, or tag in the media gallery (§10.3). |
| **"Sync corrupted my tree / created ghost records"** — two-way cloud sync with destructive merge behaviour | Family Tree Maker ↔ Ancestry | No two-way sync. Local-first SQLite is the single source of truth. External data enters only via the staging queue as proposals — never written directly. Corruption is structurally impossible. GEDCOM merge import (§7.2) lets you diff and selectively import from external files — per-assertion granularity, not destructive whole-tree merge. |
| **"I can't model my family accurately"** — rigid husband/wife templates, no support for same-sex couples, single parents, trans ancestors, non-nuclear families | Ancestry, MyHeritage, Legacy | `Family` (grouping) and `Relationship` (pairwise, typed) are separate entities. Any two persons can have any typed relationship. `ChildLink` supports biological/adopted/foster/step/unknown. No gendered templates. |
| **"D3 breaks when cousins marry"** — pedigree collapse crashes tree visualisations or duplicates nodes | Most D3.js-based tools, Ancestry's tree view | Cytoscape.js handles arbitrary DAG topologies natively. A person appears once in the graph regardless of how many ancestor paths converge on them. |
| **"I'm terrified of deleting data"** — destructive merges and edits with no undo, no way to test a hypothesis without committing | Gramps, Ancestry, RootsMagic | Research sandboxes (§3.8): lightweight overlay branches for hypothesis testing. Append-only audit log enables full undo. Assertions are never deleted, only superseded. |
| **"I can't prove it with DNA"** — no way to formally document DNA evidence alongside documentary evidence | Ancestry (no chromosome browser), Gramps | DNA matches are a source type. `CitationRef` can reference a `DnaMatch` entity. Evidence classification uses the same Mills taxonomy (Direct/Indirect/Negative) as documentary sources. Chromosome painter in Phase 4+. |
| **"My data is held hostage by a subscription"** — losing access to attached media and sources when a subscription lapses | Ancestry, MyHeritage, FindMyPast | Everything is local. SQLite database + media files on your filesystem. GEDCOM and JSON export at any time. No account, no subscription, no cloud dependency for core functionality. |
| **"I can't tell what's proven vs what's a guess"** — all data presented with equal authority regardless of sourcing quality | Ancestry, FTM, most tools | Every fact is a probabilistic assertion with confidence score, evidence type, and source citations. UI uses visual language (solid/dashed lines, colour gradients) to distinguish confirmed from proposed data. |
| **"Handwritten records are the bottleneck"** — hours spent manually transcribing parish registers, census pages, wills | All tools (no built-in OCR) | Document processing pipeline (§8): upload image → LLM vision extraction → structured assertions → staging queue → human review. AI drafts the transcription; the user corrects and confirms. |

---

#### Design Principle: Components First, Agents Later

The system is built as independent, well-bounded components with stable public interfaces. AI agents are external consumers of the same API available to any client. The core must be fully functional without any agent running.

The staging queue and event system are the integration points. Anything — a Python agent, a shell script, a user clicking "suggest match" — can submit proposals. The core validates, the human reviews.

---

#### Foundational Data Principles

Three rules that govern how data flows through the entire system. Every subsystem — storage, API, UI, import/export, agents — must respect these.

##### Principle 1: Assertions Are Canonical

The **assertions table is the single source of truth** for all genealogical facts. Entity records (the `persons`, `families`, `events` etc. tables with their JSON `data` columns) are **materialised projections** — derived views rebuilt from assertions for query performance and UI convenience.

Consequences:
* A mutation to a fact (birth date, name, relationship) is always an assertion operation: create, confirm, dispute, reject, or supersede.
* Entity JSON snapshots are recomputed from the current set of `Confirmed` + `preferred` assertions. The snapshot is a cache, not a source.
* If assertions and an entity snapshot ever disagree, the assertions win. The snapshot is regenerated.
* Exports, search indexes, timelines, graph views, and UI summaries are derived from assertions (directly or via the snapshot cache). They are never primary storage.
* Import pipelines (GEDCOM, Gramps XML, CSV) create assertions, which in turn trigger snapshot recomputation.

##### Principle 2: Family / Relationship / Event Invariants

`Family`, `Relationship`, and `Event` serve distinct, non-overlapping purposes. Without clear boundaries, the same fact (e.g., a marriage) could be recorded in three places with drift between them. These invariants prevent that:

| Entity | Role | Canonical for | Not canonical for |
|---|---|---|---|
| `Event` | **Dated evidence occurrence** — something happened at a time and place, attested by a source | The historical fact: date, place, participants, source citations | Semantic meaning of the relationship between participants |
| `Relationship` | **Semantic edge** — a typed, directed link between two persons | The nature of the connection: couple, parent-child, godparent, guardian, etc. | When/where the relationship was established (that's an Event) |
| `Family` | **Grouping container** — collects a partnership and its children for UI display and GEDCOM compatibility | Household structure, child ordering, lineage types | The partnership itself (that's a Relationship) or its date (that's an Event) |

**Linking rules:**
* A **marriage** is an `Event` (type: Marriage) with two `Principal` participants. It is the evidential record.
* The same two persons have a `Relationship` (type: Couple) that references the marriage Event as supporting evidence. The Relationship is the semantic edge.
* A `Family` references the couple Relationship (not the Event directly) and lists children. The Family is the UI/GEDCOM grouping.
* **Direction of derivation:** Event → Relationship → Family. Events are created from sources. Relationships reference Events. Families reference Relationships.
* A `Relationship` may exist without a supporting Event (e.g., an inferred parent-child link with no birth record found yet).
* A `Family` may exist without an explicit couple Relationship (e.g., a single-parent household, or a GEDCOM import where only the FAM record exists).
* **No redundant assertions:** A marriage date is asserted once, on the marriage Event. The Relationship and Family do not carry their own date assertions — they derive display dates from their linked Event(s).

##### Principle 3: Projection Rule

The following are **derived projections**, never primary storage:
* Entity JSON snapshots (§4.2)
* GEDCOM/JSON/CSV exports (§7)
* FTS5 search index (§4.6)
* UI summaries, timelines, pedigree charts, graph views (§10)
* Generated columns on entity tables (§4.2)

Any of these can be rebuilt from assertions + entity metadata without data loss. If a projection is stale or corrupt, regeneration from assertions is the fix.

```
                    ┌──────────────────────┐
                    │   Tauri Desktop App   │
                    │   (Svelte 5 + viz)    │
                    └──────────┬───────────┘
                               │ Tauri IPC
                    ┌──────────┴───────────┐
                    │     Rust Core         │
                    │  ┌────────────────┐   │
                    │  │  Domain Model  │   │
                    │  └───────┬────────┘   │
                    │  ┌───────┴────────┐   │
                    │  │  Storage Layer │   │
                    │  └───────┬────────┘   │
                    │  ┌───────┴────────┐   │
                    │  │   REST API     │◄──── External clients (agents, CLI, scripts)
                    │  │  (Axum/utoipa) │   │
                    │  └───────┬────────┘   │
                    │  ┌───────┴────────┐   │
                    │  │ Event Bus +    │   │  Proposals in, decisions out
                    │  │ Staging Queue  │   │  Events: entity created/updated/deleted
                    │  └────────────────┘   │
                    └──────────────────────┘
```

---

#### 0. Repository Structure (Monorepo)

Single repo, Cargo workspace for Rust crates, `uv` workspace for Python agents.

```
rustygene/
├── Cargo.toml                  # Rust workspace root
├── crates/
│   ├── core/                   # Domain model, assertion engine, constraints
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── storage/                # SQLite + trait abstraction
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── gedcom/                 # GEDCOM 5.5.1/7.0 import/export
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── api/                    # Axum REST API + OpenAPI spec generation
│   │   ├── Cargo.toml
│   │   └── src/
│   └── connectors/             # FamilySearch, Discovery API clients
│       ├── Cargo.toml
│       └── src/
├── app/                        # Tauri desktop application
│   ├── src-tauri/              # Rust Tauri backend (thin — delegates to crates/)
│   │   ├── Cargo.toml          # workspace member, depends on core/storage/api
│   │   └── src/
│   ├── src/                    # Svelte 5 frontend
│   │   ├── lib/
│   │   └── routes/
│   ├── package.json
│   └── vite.config.ts
├── agents/                     # Python agent workers (uv workspace)
│   ├── pyproject.toml          # workspace root
│   ├── packages/
│   │   ├── client/             # Auto-generated from OpenAPI spec
│   │   ├── agent-base/         # Shared infra: event subscription, health, config
│   │   ├── validator/          # Constraint checking agent
│   │   ├── discoverer/         # Upstream record matching agent
│   │   └── doc-processor/      # Vision/OCR extraction agent
│   └── ...
├── spec/                       # Generated OpenAPI spec (committed, used for codegen)
│   └── openapi.json
├── docs/
│   └── INITIAL_SPEC.md
├── migrations/                 # SQLite schema migrations
├── testdata/                   # Sample GEDCOM files, test fixtures
└── CLAUDE.md
```

**Workspace boundaries:**
* Rust crates depend only downward: `app/src-tauri` → `api` → `storage` → `core`. No circular dependencies.
* `core` has zero external dependencies beyond `serde`, `uuid`, `chrono`. It is the pure domain model.
* `storage` depends on `core` + `rusqlite`/`sqlx`. Nothing else.
* `api` depends on `core` + `storage` + `axum` + `utoipa`.
* Python agents depend only on the generated OpenAPI client. They never import Rust code.
* The OpenAPI spec (`spec/openapi.json`) is the contract between Rust and Python. It is generated by the `api` crate and committed to the repo.

**Future extensibility:**
* **Mobile app (iOS/Android):** Tauri 2.x supports mobile targets. The Svelte frontend and Rust core work unchanged. Or: the REST API enables a native mobile client consuming the same endpoints.
* **Cloud/shared storage:** The `storage` trait abstraction allows adding a PostgreSQL + S3 backend without touching `core`, `api`, or the frontend. The API layer is already network-ready.
* **Web app (zero-code fork):** Because the Svelte frontend communicates with the core via standard HTTP/WebSocket (§2.1), not Tauri-specific IPC, the same frontend codebase deploys as a web application with no modifications. In desktop mode, the frontend hits `http://127.0.0.1:<port>`. In web mode, the frontend hits a remote Axum server. The only differences are: (a) authentication — web mode requires OAuth2/JWT (Phase 5), desktop mode is trusted; (b) media storage — web mode uses S3 or equivalent, desktop mode uses local filesystem; (c) Tauri-specific features (system tray, native file dialogs) degrade gracefully when `window.__TAURI__` is absent. This means a web edition does not require a separate frontend codebase, a separate API, or a separate deployment pipeline — it is the same application with different configuration.

---

#### 1. Language Choice Rationale

##### Why Rust for the core (not Python)

**Python would be faster to develop.** Pydantic models are terse, GEDCOM parsing libraries are more mature in Python, and a FastAPI + SQLite prototype could exist in weeks. One language for core + agents would simplify the stack. This was seriously considered.

**Rust was chosen for these reasons:**

1. **Tauri requires it.** Tauri's backend is Rust — IPC between the webview and system layer is via Rust functions. Going Python means either a sidecar process (awkward IPC, two processes to manage, latency) or abandoning Tauri for Electron/similar (100MB+ binary, 50MB+ memory). Rust + Tauri produces a ~10MB binary with ~8MB runtime memory and instant startup.

2. **Type safety pays for itself in this domain.** Genealogy has complex invariants: imprecise dates, multiple name types, assertion confidence, pedigree collapse, temporal place validity. Rust enums and exhaustive pattern matching catch modelling errors at compile time:
   ```rust
   /// Calendar system for historical accuracy. Julian/Gregorian switchover
   /// dates vary by country (1582 Spain, 1752 Britain, 1918 Russia, 1923 Greece).
   /// Records should preserve the original calendar; conversion is display-layer.
   enum Calendar {
       Gregorian,     // default, post-switchover
       Julian,        // pre-switchover (country-dependent)
       DualDate,      // e.g. "3 February 1723/24" (Old Style year + New Style year)
       Hebrew,        // ketubot, tombstones
       FrenchRepublican, // 1793-1805
       Islamic,       // Hijri calendar
   }

   enum DateValue {
       Exact { date: NaiveDate, calendar: Calendar },
       Range { from: NaiveDate, to: NaiveDate, calendar: Calendar },
       Before { date: NaiveDate, calendar: Calendar },
       After { date: NaiveDate, calendar: Calendar },
       About { date: NaiveDate, calendar: Calendar },
       Tolerance { date: NaiveDate, plus_minus_days: u32, calendar: Calendar }, // "1868 +/- 1 year"
       Quarter { year: i32, quarter: u8 },
       Textual(String), // unparseable dates preserved verbatim from source
   }
   ```
   Every consumer of `DateValue` must handle all variants. In Python, this is a runtime check that can be missed.

3. **SQLite integration is mature.** `rusqlite` provides zero-overhead C bindings. `sqlx` provides compile-time query checking. Both are production-grade.

4. **The Rust layer is kept thin.** The pain of Rust verbosity is managed by:
   - Using JSON columns for flexible entity data (`data JSON NOT NULL`). Adding a field to `Person` means changing one struct + one `#[serde(default)]` — no schema migration.
   - Deriving everything (`serde`, `Clone`, `Debug`, `PartialEq`).
   - Keeping the domain model in `crates/core` with zero dependencies beyond `serde`/`uuid`/`chrono`.
   - Using Rust enums aggressively for the domain, where the type system genuinely helps.
   - Not fighting messy text in Rust — GEDCOM import can preprocess via a thin Python step if needed.

5. **Clean language boundary.** Rust = authority (storage, validation, constraints, API). Python = advisory (AI agents proposing mutations). This separation is architecturally meaningful, not accidental.

##### Why Python for agents (not Rust)

AI agent work is inherently exploratory and fast-changing. LLM SDK APIs shift frequently. Pydantic AI, `google-genai`, `anthropic` SDKs are Python-first. Agent logic benefits from rapid iteration, not compile-time safety. The REST API boundary means agents are just HTTP clients — language doesn't matter, but Python has the best ecosystem for this work.

---

#### 2. System Architecture

A decoupled, component-oriented architecture separating strict deterministic storage from probabilistic AI inference.

* **Core Logic & Storage Layer:** Rust library crates. Source of truth. Enforces biological, chronological, and geographic constraints. Pure domain model with no framework dependencies.
* **Embedded HTTP Server:** Axum + utoipa. Starts on application launch and binds to `127.0.0.1` on a local port. The single integration point for all consumers — the Svelte frontend, agents, CLI, scripts. OpenAPI spec auto-generated; client SDKs derived from spec. See §2.1 for rationale.
* **Event Bus:** Internal pub/sub system for entity lifecycle events (created, updated, deleted, proposal_submitted, proposal_reviewed). Agents subscribe to relevant events via SSE streams or polling. Enables agents to react to changes without coupling to the core.
* **Presentation Layer:** Tauri 2.x desktop application (macOS initially). Svelte 5 frontend with Cytoscape.js and D3.js for graph rendering. Communicates with the core via the embedded HTTP server, not Tauri IPC (see §2.1).
* **Agent Workers:** External processes (any language — see §11.4) that consume the HTTP API and event bus. Pluggable — zero, one, or many agents can run independently. Each writes proposals to the staging queue, never directly to the graph.

##### 2.1 Embedded Axum Server vs Tauri IPC

Tauri's native IPC (`invoke`) requires all data to be serialized to strings and deserialized back on the other side. For small payloads (window management, file dialog results) this is fine. For data-heavy operations — rendering a pedigree graph with thousands of nodes, streaming search results, bulk import progress — the serialization overhead causes measurable UI stutter and thread blocking.

The architecture avoids this bottleneck by running an embedded Axum HTTP server inside the Tauri process:

* **Data-heavy traffic** (entity CRUD, graph queries, search, import/export progress, SSE event streams) flows through the local HTTP server. The Svelte frontend talks to `http://127.0.0.1:<port>` via standard `fetch`/`EventSource` — no IPC bridge involved.
* **Native OS operations** (window management, system tray, file open/save dialogs, clipboard, notifications) use Tauri IPC commands. These are small, infrequent payloads where IPC overhead is negligible.
* **WebSocket** for high-frequency updates: the embedded server exposes a WebSocket endpoint for real-time graph updates during import, agent processing, or collaborative sessions (Phase 5).

This yields three structural advantages:
1. **No serialization bottleneck** for large payloads — raw HTTP transfer outperforms string-serialized IPC.
2. **Zero-code web deployment** — the same Svelte frontend works as a desktop app (hitting `localhost`) or as a web SaaS (hitting a remote server) with no codebase forking.
3. **Surface area reduction** — the Rust↔JS bridge is minimised to native OS calls. Debugging is simpler; the API is testable with `curl`.

The embedded server starts in Phase 2 (desktop app). In Phase 3, the same server is exposed for external agent access. No code duplication — the only change is binding configuration and authentication requirements.

---

#### 3. Data Model (Probabilistic Assertions)

Traditional genealogy databases treat data as binary facts. This system models data as assertions with varying confidence levels, supporting multiple competing assertions per field.

##### 3.1 Entity Types

The data model draws from three established models, taking the best of each:

* **GEDCOM X** (FamilySearch): Person, Relationship (Couple / ParentChild), Fact, SourceDescription, Agent. Flat relationship model — no "Family" grouping object, just pairwise relationships. Every conclusion has Attribution (who, when, why). Identifiers can be Primary, Authority, or Deprecated (for merges).
* **Gramps** (v5.2): Person, Family, Event, Place, Source, Citation, Repository, Media, Note as primary objects. Family is a first-class grouping object linking partners + children. Events are shared across people (a census record is one Event, multiple people participated). Places have hierarchical names.
* **GEDCOM 5.5.1/7.0**: The interchange standard. Person (INDI), Family (FAM), Source (SOUR), Repository (REPO), Note (NOTE), Media (OBJE). Family-centric model.

**RustyGene's model** follows the Gramps approach (Family as a first-class grouping) while incorporating GEDCOM X's attribution and pairwise relationship richness:

| Entity | Purpose | Key properties | Analogues |
|---|---|---|---|
| `Person` | An individual | names (ordered, typed: birth/married/aka), gender, living flag, private flag | GEDCOM X Person, Gramps Person, GEDCOM INDI |
| `Family` | A partnership/union grouping | partner refs, child refs (with lineage type: biological/adopted/foster/step/unknown), relationship type | Gramps Family, GEDCOM FAM |
| `Relationship` | A pairwise link between two persons | type (couple/parent-child/godparent/guardian/...), person1 ref, person2 ref, facts | GEDCOM X Relationship — more expressive than Family alone |
| `Event` | Something that happened | type (birth/death/marriage/census/baptism/burial/migration/occupation/...), date, place ref, participants (person refs with typed roles — see below) | Gramps Event, GEDCOM X Fact |
| `Place` | A location with temporal validity | See §3.1.2 Place Model below | Gramps Place (GEPS 045), GEDCOM X PlaceDescription |
| `Repository` | Where sources are held | name, address, URLs, type (archive/library/website/personal collection) | Gramps Repository, GEDCOM REPO |
| `Source` | A document or record set | title, author, publication info, abbreviation, repository refs (with call number + media type per ref) | Gramps Source, GEDCOM X SourceDescription |
| `Citation` | A specific reference within a source | volume/page/folio/entry, confidence level, date accessed, transcription | Gramps Citation, GEDCOM X SourceReference |
| `Media` | An attached file | file path, content hash, mime type, thumbnail, OCR text, dimensions | Gramps Media, GEDCOM OBJE, FamilySearch Memory |
| `Note` | Free-text annotation | text (markdown), type (research/transcript/general), linked entity refs | Gramps Note, GEDCOM NOTE, FamilySearch Discussion |
| `LdsOrdinance` | LDS temple ordinance | type (baptism/endowment/seal-to-parents/seal-to-spouse/confirmation), status (20+ values), temple code, date, place ref, family ref (for sealings) | Gramps LdsOrd, GEDCOM BAPL/ENDL/SLGC/SLGS |

**Design notes:**
* Both `Family` (grouping) and `Relationship` (pairwise) exist because they serve different purposes. A Family groups a household for GEDCOM compatibility and UI display. A Relationship captures precise typed links (e.g., godparent, guardian) that don't fit the nuclear family model.
* Events are shared objects — a census record is one Event with multiple Person participants (each with a role: head, wife, child, servant, etc.). This matches how records actually work. **Citation propagation:** When an Event is created with a source citation and multiple participants, the citation automatically propagates to every participant's assertions in the same transaction. A user entering a census page with 6 household members creates one Event, one Citation, and one set of assertions — not 6 copies of the same citation. This directly eliminates the "citation slog" that genealogists cite as the single most demoralising part of data entry.
* **Three-tier citation model** (following Gramps and Mills' *Evidence Explained*): `Repository` → `Source` → `Citation`. A Repository is where you go (The National Archives, Ancestry.com). A Source is what you find there (1881 England Census, a parish register). A Citation is the specific reference (page 42, entry 17, the exact transcription). Cardinality: many Citations → one Source → many Repositories (a source may be available at multiple repositories, each with its own call number). Nearly every entity can carry citation refs. This separation is critical for GPS compliance — it enables complete, reproducible source trails.
* **Media region references:** A `MediaRef` linking any entity to a `Media` item carries an optional crop rectangle (4-tuple of percentages: x, y, width, height). A group photo can be referenced by many Person objects, each highlighting a different face. This follows the Gramps MediaRef model.
* **Event participant roles** are typed, not free-text: `Principal` (the person the event is about), `Witness`, `Godparent`, `Informant`, `Clergy`, `Registrar`, `Celebrant`, `Parent`, `Spouse`, `Child`, `Servant`, `Boarder`, `Custom(String)`. Witnesses and godparents are valuable relationship evidence — a recurring witness across events implies a close relationship. Census roles (Head, Wife, Son, Daughter, Servant, Boarder, Visitor, ...) are modelled separately as `CensusRole`.
* **Ad-hoc notes on any entity:** Every primary object (Person, Family, Event, Place, Source, Citation, Repository, Media) and most secondary objects (assertions, event refs, media refs, names) can carry `Vec<NoteRef>`. Notes are first-class entities with their own UUIDs, supporting markdown text, typed categorisation (General, Research, Transcript, SourceText, Todo, Custom), and full citation attachment. This is the escape hatch for anything the structured model cannot capture.
* **Living persons:** The `living: bool` flag on Person controls privacy. Detection heuristic: no death event and birth < 100 years ago (configurable threshold). Export functions redact living persons by default (name → "Living", events stripped, node preserved for structure). The `private` flag is a stricter per-object override — anything marked private is excluded from all exports regardless of living status. GDPR-relevant for a future server edition.
* **LDS ordinances** are modelled as a **dedicated secondary object** on Person and Family, not as generic Events. They carry domain-specific fields (temple code, 20+ LDS-specific status values like BIC/DNS/Completed/Submitted, family handle for sealings) that don't map to the Event model. This follows the Gramps `LdsOrd` pattern and is required for lossless GEDCOM round-trip because GEDCOM 5.5.1 encodes these as `BAPL`, `ENDL`, `SLGC`, `SLGS` tags with structured sub-records.
* **GEDCOM round-trip fidelity (phased):** The goal is lossless round-trip for all standard GEDCOM 5.5.1 tags, but this is achieved incrementally — not as a day-one hard gate. Data that our model cannot natively represent (custom tags, non-standard extensions) is preserved verbatim in a `_raw_gedcom: Map<String, String>` escape hatch on each entity. This ensures no user data is silently dropped.
    * **Phase 1A — Semantic fidelity:** Core records (INDI, FAM, SOUR, REPO, NOTE, OBJE) import and export with correct meaning. Field values, relationships, and citations are preserved. Unknown/custom tags round-trip via `_raw_gedcom`. Whitespace, line-wrapping, tag ordering, and vendor-specific formatting quirks are *not* guaranteed to be identical. The test bar: import a reference file, export it, re-import — the assertion graph is identical.
    * **Phase 1B — Tag coverage and corpus testing:** Expand the round-trip test suite to cover all standard 5.5.1 tags (including less common ones like ASSO, SOUR.DATA, CHAN). Test against a corpus of real-world GEDCOM files from major vendors (Ancestry, FamilyTreeMaker, Gramps, RootsMagic, Legacy). Fix tag-level gaps found by corpus testing.
    * **Phase 3+ — Textual fidelity:** Where feasible, preserve ordering, formatting, and structure so that a diff of input vs output is minimal. "Any delta is a bug" becomes the bar only after corpus testing proves it achievable without turning the importer/exporter into the whole project.
* **Pedigree collapse:** The person graph is a **DAG**, not a tree. A person is stored once but may appear at multiple positions in an ahnentafel. Cousin marriages and endogamous communities (Ashkenazi Jewish, colonial New England, rural Scandinavia, etc.) produce pervasive collapse. Cytoscape.js handles arbitrary topologies. Standard cM-to-relationship estimation tables assume no endogamy — agents doing DNA matching must account for inflated shared cM values in endogamous lines.

##### 3.1.1 Place Model (Hierarchical, Temporal)

Places are first-class entities with multi-layered hierarchy and date-bounded names, following the Gramps GEPS 045 model:

```rust
struct Place {
    id: Uuid,
    place_type: PlaceType,    // Country, State, County, Parish, Town, Village, Farm, Cemetery, ...
    names: Vec<PlaceName>,     // multiple names with date ranges and language codes
    coordinates: Option<(f64, f64)>, // lat/lon (WGS84)
    enclosed_by: Vec<PlaceRef>, // parent places — can have multiple simultaneous hierarchies
    external_ids: Vec<ExternalId>, // GeoNames ID, Getty TGN ID, ISO 3166 code
}

struct PlaceName {
    name: String,
    language: Option<String>,   // ISO 639-1 (en, de, ru, ...)
    date_range: Option<DateRange>, // when this name was valid
}

/// A place can belong to multiple hierarchies simultaneously (administrative,
/// religious, geographic, judicial). Each link has its own date range —
/// jurisdictions change over time (Alsace: France↔Germany six times).
struct PlaceRef {
    place_id: Uuid,            // parent place
    hierarchy_type: HierarchyType, // Admin, Religious, Geographic, Judicial, Cultural
    date_range: Option<DateRange>, // when this enclosure was valid
}
```

**Key design decisions:**
* Record the place name **as it appeared in the source** (e.g., "Königsberg"), link to the modern equivalent ("Kaliningrad") via an enclosed-by chain or external ID. Never silently modernise.
* Title generation for events follows the hierarchy valid at the event's date. A birth in "Breslau" in 1890 displays as "Breslau, Schlesien, Preußen" — not "Wrocław, Dolnośląskie, Poland".
* **GeoNames** (12M names, open data, REST API) and **Getty TGN** (2.4M records, polyhierarchy) are integration targets for place authority lookups. Phase 3+.
* Places carry a `private` flag like all entities.

**Phase 1A simplification:** The full temporal polyhierarchical model is the target architecture, but Phase 1A implements only a subset to reduce risk:
* `Place` stores: a single primary name (as it appeared in the source), `place_type`, optional coordinates, optional single `enclosed_by` parent (one hierarchy, no date range), optional `external_ids`.
* `PlaceName.date_range` is always `None` in Phase 1A. The field exists in the Rust struct but storage and import do not populate it.
* `PlaceRef.hierarchy_type` defaults to `Admin` and `PlaceRef.date_range` is always `None`.
* Multiple `enclosed_by` entries (polyhierarchy) and date-bounded names activate in Phase 1B/2.
* The Rust types in `crates/core` define the full model from day one — the simplification is in what storage and import populate, not in the type definitions. This avoids schema changes later.

##### 3.1.2 Cross-Tree Linking (Future)

For tree sharing and collaboration:

* **External identifiers:** Every entity can carry external IDs (GEDCOM X `Identifier` with types: `Primary`, `Authority`, `Deprecated`). An Authority identifier links to a person in an external system (FamilySearch PID, another RustyGene instance, WikiTree ID).
* **Linked trees:** A user can link a local Person to an external tree's Person via an Authority identifier. The link carries a `trust_level` (full / partial / untrusted) that governs how much data is imported:
    * `full`: All assertions from the linked tree are imported with the external tree's confidence scores.
    * `partial`: Only assertions with source citations are imported; unsourced assertions are discarded.
    * `untrusted`: Link is recorded for reference only; no assertions are imported.
* **Trust propagation:** Assertions originating from a linked tree carry a `provenance` field identifying the external source. If the trust level is later downgraded, imported assertions can be bulk-reassessed.
* **Merge support:** When two Person records are determined to be the same individual (local dedup or cross-tree match), one becomes the primary and the other is marked `Deprecated`. All references are rewritten. The audit log records the merge for reversal. The entity matching engine (§7.2) provides the scoring infrastructure for both interactive merge import and automated dedup.
* **Unmerge support:** Merges can be reversed. The audit log stores the complete pre-merge state of both entities. Unmerge restores both persons to their pre-merge state and rewrites references back. This is messy (assertions may have been added post-merge that reference both original entities) — the UI flags these for manual review after unmerge.

##### 3.1.3 DNA Data Model (Phase 4+)

DNA evidence is increasingly central to genealogy. The data model must accommodate it even if integration is deferred.

```rust
struct DnaTest {
    id: Uuid,
    person_id: Uuid,               // the person tested
    test_type: DnaTestType,        // Autosomal, YDNA, MtDNA, XDNA
    provider: String,              // "AncestryDNA", "23andMe", "FamilyTreeDNA", "MyHeritage"
    kit_id: Option<String>,        // provider-specific kit number
    haplogroups: Option<Haplogroups>,
    ethnicity_estimate: Option<Json>, // provider-specific, schema varies, changes over time
    import_date: DateTime<Utc>,
}

struct DnaMatch {
    id: Uuid,
    test_id: Uuid,                 // the local test
    matched_person_id: Option<Uuid>, // linked local person (if identified)
    matched_name: String,          // display name from provider
    shared_cm: f64,                // total shared centiMorgans
    shared_segments: u32,          // number of shared segments
    longest_segment_cm: Option<f64>,
    estimated_relationship: Option<String>, // provider's estimate
    segments: Option<Vec<DnaSegment>>,      // chromosome-level detail (if available)
}

struct DnaSegment {
    chromosome: u8,                // 1-22 or 23 (X)
    start_position: u64,           // base pair
    end_position: u64,
    cm: f64,                       // centiMorgans for this segment
    snps: Option<u32>,
}

struct Haplogroups {
    y_dna: Option<String>,         // e.g. "R-M269"
    mt_dna: Option<String>,        // e.g. "H1a1"
}
```

**Key design decisions:**
* Segment data enables **triangulation** — proving a shared ancestor by confirming multiple matches share the same chromosome segment. Not all providers expose this (AncestryDNA does not).
* Ethnicity estimates are stored as opaque JSON because they change as providers update reference panels. They are informational, not evidential.
* Shared cM values must be interpreted with **endogamy awareness** — standard cM-to-relationship tables (Shared cM Project v4.0) assume outbred populations. Agents doing relationship prediction must flag endogamous lines.
* DNA tables are separate from the core entity tables. DNA is evidence that feeds into assertions, not a core entity type.
* **DNA as evidence source:** `EvidenceType` (Direct/Indirect/Negative) is Mills' taxonomy of *reasoning methodology* and must not be conflated with source type. DNA evidence uses the existing taxonomy: a shared 1500cM segment is *direct* evidence of a parent-child relationship; a 45cM match is *indirect* evidence requiring inference. To link DNA results to assertions, `CitationRef` can reference a `DnaMatch` entity as its source (via a `source_type: "dna_match"` discriminator on the citation). This enables assertions like "John is the biological father of Mary (evidence: DnaMatch #xyz, 3400cM shared, direct evidence)" without polluting the Mills framework with a fourth category.

##### 3.2 Assertion Wrapper

Every fact and relationship implements a probabilistic assertion:

```rust
pub struct Assertion<T> {
    pub id: Uuid,
    pub value: T,
    pub confidence: f64,              // 0.0..=1.0
    pub status: AssertionStatus,       // Confirmed, Proposed, Disputed, Rejected
    pub evidence_type: EvidenceType,   // Direct, Indirect, Negative (Mills taxonomy)
    pub source_citations: Vec<CitationRef>,
    pub proposed_by: ActorRef,         // user:<id>, agent:<name>, import:<job_id>
    pub created_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub reviewed_by: Option<ActorRef>,
}

/// Mills' Evidence Explained taxonomy.
/// Direct: answers the question without inference (birth certificate names parents).
/// Indirect: requires inference from multiple pieces (census age implies birth year).
/// Negative: reasoning from absence — a record that SHOULD exist does not (distinct
/// from a negative search result, which is a research log entry, not evidence).
enum EvidenceType { Direct, Indirect, Negative }
```

Multiple competing assertions can coexist for the same field (e.g., two possible birth dates from conflicting sources). The human resolves conflicts via the review queue.

##### 3.3 Assertion Conflict Resolution

When multiple `Confirmed` assertions exist for the same entity + field:
* **UI:** Displays all competing assertions with confidence scores, sources, and provenance. The user can mark one as `preferred`.
* **Default query behaviour:** Returns the `preferred` assertion if set, otherwise the highest-confidence `Confirmed` assertion. API callers can request all assertions via `?include_all=true`.
* **Agent proposals:** A new proposal for an already-asserted field does not replace the existing assertion. It enters the staging queue for human review. The user decides whether to confirm, dispute, or reject.

##### 3.4 Research Log

A structured research log — a significant gap in all major genealogy tools. Tracks what was searched, where, and what was (or was not) found. Critical for GPS compliance (element 1: "reasonably exhaustive search") and for preventing duplicate effort.

```rust
struct ResearchLogEntry {
    id: Uuid,
    date: DateTime<Utc>,
    objective: String,                 // "Find birth record for John Smith b. ~1850"
    repository: Option<Uuid>,          // link to Repository entity
    repository_name: Option<String>,   // free text if no Repository entity exists
    search_terms: Vec<String>,         // what was searched
    source_searched: Option<Uuid>,     // link to Source entity if applicable
    result: SearchResult,              // Found, NotFound, PartiallyFound, Inconclusive
    findings: Option<String>,          // what was found (or why nothing was found)
    citations_created: Vec<Uuid>,      // links to Citation entities created from this search
    next_steps: Option<String>,        // what to try next
    person_refs: Vec<Uuid>,            // persons this research relates to
    tags: Vec<String>,
}

enum SearchResult { Found, NotFound, PartiallyFound, Inconclusive }
```

**Key distinction:** A `NotFound` result is a research log entry, not negative evidence. Negative evidence (§3.2) requires establishing that a record *should* exist and reasoning from its absence — a higher bar that produces an `Assertion` with `evidence_type: Negative`.

**UI bridge between NotFound and Negative Evidence:** When a user (or agent) logs a `NotFound` result for a specific person in a specific source (e.g., "searched 1871 census for Thomas Jones in Llanelly — not found"), the UI should prompt: *"Thomas Jones is not recorded in the 1871 Llanelly census. Do you want to assert that Thomas Jones was not living in Llanelly in 1871?"* Accepting creates a `Negative` evidence assertion linked to the research log entry. This turns failed searches into structured evidence that eventually proves migration timelines or identity disambiguation. This is one of the most powerful analytical workflows in genealogy and no current tool automates it.

##### 3.5 Genealogical Proof Standard (GPS) Alignment

The architecture maps to the BCG's five GPS elements:

| GPS Element | RustyGene Support |
|---|---|
| 1. Reasonably exhaustive search | Research log tracks all searches including negative results. Agents can flag under-researched persons. |
| 2. Complete, accurate citations | Three-tier citation model (Repository → Source → Citation) with structured fields. |
| 3. Analysis and correlation of evidence | Multiple assertions per field with evidence type classification (direct/indirect/negative). |
| 4. Resolution of conflicting evidence | Assertion conflict resolution (§3.3) with explicit dispute/preferred workflow. |
| 5. Soundly reasoned written conclusion | Notes (type: "Research Conclusion") attached to entities or assertions, supporting markdown. |

GPS is a methodology, not a data structure — the system provides infrastructure for disciplined researchers, not enforcement.

##### 3.6 Authentication & Authorization

* **Desktop mode (Phase 1-2):** No user authentication. Single-user, local SQLite, all access is trusted.
* **Agent API keys (Phase 3):** API keys generated via CLI (`rustygene agent register <name>`) or UI. Each agent authenticates with `Authorization: Bearer <key>`. Keys are stored hashed in the `agents` table. Rotation via `rustygene agent rotate <name>`.
* **Future server mode (Phase 5):** OAuth2 + JWT for multi-user collaboration. Per-user permissions (read-only, propose, review, admin). Agents authenticate as service accounts.
* **Secret storage:** LLM API keys, external service credentials (FamilySearch, Ancestry), and agent API keys are stored in the OS-native secure keystore — **not** in the SQLite database, not in environment variables, not in plaintext config files. On macOS: Keychain Services. On Windows: Credential Manager. On Linux: Secret Service API (via `libsecret`/`kwallet`). The Rust `keyring` crate provides a cross-platform abstraction. The CLI manages secrets: `rustygene secret set openai-api-key`, `rustygene secret list`, `rustygene secret delete <name>`. Agents request credentials from the core via the API (`GET /config/secrets/{name}`) — the secret is returned in-process, never logged, never serialised to disk.

##### 3.6.1 Embedded Server Security

The embedded Axum HTTP server (§2.1) introduces security considerations that must be addressed from Phase 2:

* **Bind to loopback only:** The server binds to `127.0.0.1:<port>`, never `0.0.0.0`. This ensures the API is physically accessible only to processes on the local machine. No LAN or internet exposure.
* **CORS policy:** The server's CORS configuration explicitly whitelists the Tauri `tauri://` and `https://tauri.localhost` protocol origins used by the WebView in production, plus `http://localhost:<port>` for development. All other origins are rejected. This prevents arbitrary browsers or malicious local scripts from issuing API requests.
* **Tower middleware stack:** Axum's Tower integration is used for:
    * Request tracing and structured logging (via `tower-http::trace`).
    * Security headers (`X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`).
    * Rate limiting (optional, primarily for agent API access in Phase 3+).
    * Request size limits to prevent memory exhaustion from malformed payloads.
* **Port selection:** On startup, the server selects an available port (or uses a user-configured fixed port). The port is communicated to the Tauri WebView via an IPC init command. This avoids port conflicts with other local services.
* **Unix domain sockets (alternative):** On macOS/Linux, the server can optionally bind to a Unix domain socket (e.g., `~/.rustygene/rustygene.sock`) instead of a TCP port. This provides filesystem-level access control and eliminates network exposure entirely. TCP remains the default for simplicity and cross-platform compatibility.

##### 3.7 Design Constraints

* **Names are complex:** A `Person` has `Vec<PersonName>`, each with a `NameType` (Birth, Married, AKA, Immigrant, Religious, Custom) and optional date range. Each `PersonName` contains:
    * `given_names: String` — first + middle, space-separated (no separate middle name field — varies by culture).
    * `call_name: Option<String>` — the name actually used day-to-day (German Rufname, Swedish tilltalsnamn, anglicised names).
    * `surnames: Vec<Surname>` — one or more, each carrying an `OriginType` (Patrilineal, Matrilineal, Patronymic, Matronymic, Location, Occupation, Feudal, Pseudonym, Taken, Inherited, Custom) and an optional `connector` (e.g., "de", "van", "y" for compound surnames like "García y López").
    * `prefix: Option<String>` — title or honorific (Dr, Rev, Sir).
    * `suffix: Option<String>` — generational or professional (Jr, III, Esq).
    * `sort_as: Option<String>` — explicit sort key for patronymic cultures where surname-first sorting is wrong.
  This follows the Gramps multi-surname model. Patronymic systems (Icelandic, pre-1900 Scandinavian), Spanish double surnames, Chinese/Korean family-name-first order, and immigration-era anglicisation are all first-class. People have married names, maiden names, aliases, spelling variations across records.
* **Dates are imprecise and fuzzy:** Support exact, range, before/after, about, quarter (Q1 1881), textual ("between 1850 and 1855"), and **tolerance ranges** (e.g., "1868 +/- 1 year"). The `DateValue` enum encodes all variants. Fuzzy matching uses tolerance to find overlapping date ranges — a search for birth "about 1850" matches records from 1848-1852. Agents use tolerance ranges for probabilistic matching (higher tolerance = lower confidence).
* **Relationships are diverse:** Same-sex partnerships, adoption, fosterage, step-parenting — all first-class via typed `ChildLink` (biological/adopted/foster/step/unknown) and `PartnerLink` enums.
* **Pedigree collapse:** Ancestors appearing multiple times (cousin marriages) handled naturally — `Person` nodes are referenced, not duplicated. The graph may contain cycles.
* **Place temporality:** "Middlesex" doesn't exist post-1965. Places have validity date ranges and may reference successor/predecessor places.

##### 3.8 Research Sandboxes (Hypothesis Branches)

Genealogists are terrified of deleting data in case they are wrong. "What if this Thomas Jones is actually a different person?" Destructive actions (delete, merge) in a single-timeline system force premature commitment. Research sandboxes solve this.

**Concept:** A sandbox is a lightweight, named branch of the entity graph. The user can create competing hypotheses and let the validator agent score them:

```
Branch A: "Thomas (1866) belongs to the Richards family via patronymic shift"
Branch B: "Thomas (1866) belongs to the Thomas Jones (1821) / Margaret (1832) family"
```

**Implementation:**
* Each sandbox is a **set of overlay assertions** — they don't copy the entire graph, just the assertions that differ from the main branch (the "trunk").
* An assertion carries an optional `sandbox_id: Option<Uuid>`. Trunk assertions have `None`. Sandbox assertions have a sandbox UUID.
* Reading the graph with a sandbox active = trunk assertions + sandbox overlay (sandbox wins on conflict for the same entity+field).
* The validator agent can run against each sandbox independently and report which has fewer contradictions.
* When the user is satisfied, they **promote** a sandbox (its assertions become trunk) or **discard** it (overlay assertions deleted). No data is lost until the user explicitly discards.

```rust
struct Sandbox {
    id: Uuid,
    name: String,                     // "Richards patronymic hypothesis"
    description: Option<String>,
    created_at: DateTime<Utc>,
    parent_sandbox: Option<Uuid>,     // sandboxes can nest (rare but possible)
    status: SandboxStatus,            // Active, Promoted, Discarded
}

enum SandboxStatus { Active, Promoted, Discarded }
```

```sql
-- Sandbox registry
CREATE TABLE sandboxes (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL,
    parent_sandbox TEXT,
    status TEXT NOT NULL DEFAULT 'active'
);

-- Assertions gain an optional sandbox_id
-- ALTER TABLE assertions ADD COLUMN sandbox_id TEXT REFERENCES sandboxes(id);
-- CREATE INDEX idx_assertions_sandbox ON assertions(sandbox_id);
```

**UI:** Sandbox selector in the toolbar (like git branches). Active sandbox shown as a coloured banner. Side-by-side comparison view for competing hypotheses. "Score this sandbox" button runs the validator.

**Phase:** Core sandbox data model in Phase 1 (just the `sandbox_id` column on assertions). UI and validator integration in Phase 2-3. This is a massive differentiator — neither Ancestry nor Gramps support this.

---

#### 4. Storage Layer

SQLite as primary database. Trait-based abstraction for future backend swappability.

##### 4.1 Primary Storage: SQLite

SQLite is the source of truth for all entity data. Single file, ACID transactions, zero configuration.

**Why SQLite:**
* **ACID transactions.** Creating a Family that links two Persons and an Event is atomic — it all commits or none does. No partial-write corruption.
* **Referential integrity.** Foreign keys enforced by the database, not just application code.
* **Full-text search.** FTS5 built-in, no external dependency.
* **Single file.** Backup = copy one file. Trivial to understand, trivial to deploy.
* **Battle-tested.** Most deployed database on earth. Every mobile app, every browser, most desktop apps.
* **Excellent Rust support.** `rusqlite` (zero-overhead C bindings) or `sqlx` (async, compile-time query checking).
* **Performance at target scale.** 1000s of entities = single-digit MB. Queries return in microseconds.

##### 4.2 Schema Design

**Timestamps:** All `TEXT` timestamp columns use ISO 8601 format (`YYYY-MM-DDTHH:MM:SSZ`, UTC, 24-hour clock). Enforced at the application layer via `chrono::DateTime<Utc>` serialization.

**SQL schema migrations:** DDL changes (new tables, new columns, index changes) are managed by `refinery` with versioned migration files in `migrations/`. Migrations run automatically on database open. This is separate from JSON schema versioning (below), which handles changes *within* entity JSON blobs.

**JSON schema versioning:** Entity JSON blobs include a `schema_version: u32` field. On startup, the storage layer runs migration functions that upcycle older JSON payloads to the current schema version. This handles structural changes within JSON (field renames, type changes) that `#[serde(default)]` alone cannot catch.

```sql
-- All entities share a common pattern:
-- UUID primary key, created/updated timestamps, versioned JSON for flexible fields

CREATE TABLE persons (
    id TEXT PRIMARY KEY,              -- UUID
    version INTEGER NOT NULL DEFAULT 1, -- optimistic locking (incremented on each mutation)
    schema_version INTEGER NOT NULL,  -- JSON blob schema version
    data JSON NOT NULL,               -- full Person struct as JSON
    created_at TEXT NOT NULL,         -- ISO 8601 UTC
    updated_at TEXT NOT NULL          -- ISO 8601 UTC
);
-- Same pattern for families, events, places, sources, citations, repositories, media, notes.
-- All entity tables include `version` for optimistic locking and `schema_version` for JSON migrations.

-- Generated columns for high-value query fields extracted from JSON blobs.
-- Avoids full table scans while preserving JSON flexibility.
-- See: https://www.sqlite.org/gencol.html
-- Example:
--   ALTER TABLE persons ADD COLUMN birth_year INTEGER
--     GENERATED ALWAYS AS (json_extract(data, '$.birth_event.year')) VIRTUAL;
--   CREATE INDEX idx_persons_birth_year ON persons(birth_year);
-- Add generated columns for any field that appears in WHERE/ORDER BY clauses.
```

**Entity snapshot consistency (write-through):** Entity JSON snapshots (the `data` column) are recomputed on every assertion mutation — not lazily on read. When an assertion is created, confirmed, disputed, rejected, or has its preferred flag changed, the storage layer immediately rebuilds the affected entity's snapshot from the current set of `Confirmed` + `preferred` assertions and writes it back. This is a synchronous step within the same transaction as the assertion mutation. At the target scale (thousands of entities), the cost is negligible and the model is simpler to reason about than eventual consistency. If a snapshot is ever suspected stale, `rustygene rebuild-snapshots` (CLI) regenerates all snapshots from assertions.

```sql
-- Assertions stored in a unified table, linked to any entity
CREATE TABLE assertions (
    id TEXT PRIMARY KEY,
    entity_id TEXT NOT NULL,       -- FK to any entity table
    entity_type TEXT NOT NULL,     -- 'person', 'family', 'event', ...
    field TEXT NOT NULL,           -- 'birth_date', 'name', 'parent_of', ...
    value JSON NOT NULL,           -- full assertion value as JSON
    value_date TEXT,               -- extracted for date assertions (enables range queries)
    value_text TEXT,               -- extracted for name/text assertions (enables LIKE queries)
    confidence REAL NOT NULL,      -- 0.0..1.0
    status TEXT NOT NULL,          -- 'confirmed', 'proposed', 'disputed', 'rejected'
    preferred INTEGER NOT NULL DEFAULT 0, -- 1 if user explicitly marked as preferred
    source_citations JSON,         -- array of citation refs
    proposed_by TEXT NOT NULL,     -- 'user:<id>', 'agent:<name>', 'import:<job>'
    reviewed_by TEXT,
    created_at TEXT NOT NULL,      -- ISO 8601 UTC
    reviewed_at TEXT,              -- ISO 8601 UTC
    evidence_type TEXT NOT NULL DEFAULT 'direct', -- 'direct', 'indirect', 'negative' (Mills taxonomy)
    idempotency_key TEXT UNIQUE   -- hash(entity_id + field + value + sorted(source_citations))
    -- The key includes ONLY factual content: entity_id, field, value, and source_citations.
    -- It deliberately EXCLUDES metadata: confidence, status, evidence_type, proposed_by,
    -- reviewed_by, timestamps. Two proposals for the same fact from different agents (or with
    -- different confidence) are the same assertion — the second is a duplicate, not a new fact.
    -- NOTE: hash must sort arrays before hashing. [A,B] and [B,A] must produce the same key.
);

-- Optimistic locking: mutations require `WHERE version = ?`. On mismatch, return 409 Conflict.
-- UPDATE persons SET data = ?, version = version + 1 WHERE id = ? AND version = ?;

CREATE INDEX idx_assertions_entity_field ON assertions(entity_id, field);
CREATE INDEX idx_assertions_date ON assertions(value_date) WHERE value_date IS NOT NULL;
CREATE INDEX idx_assertions_status ON assertions(status);
CREATE INDEX idx_assertions_confidence ON assertions(entity_id, field, confidence DESC);

-- Graph edges for relationship traversal
CREATE TABLE relationships (
    id TEXT PRIMARY KEY,
    from_entity TEXT NOT NULL,
    from_type TEXT NOT NULL,
    to_entity TEXT NOT NULL,
    to_type TEXT NOT NULL,
    rel_type TEXT NOT NULL,        -- 'parent_of', 'partner_in', 'resided_at', ...
    assertion_id TEXT REFERENCES assertions(id),
    directed INTEGER NOT NULL DEFAULT 1 -- 1 = directed (parent_of), 0 = undirected (partner_in)
);
-- Directed relationships (parent_of, guardian_of) are queried as from→to only.
-- Undirected relationships (partner_in, sibling_of) are stored once (lower UUID first by convention)
-- and queries match on EITHER from_entity OR to_entity. The `directed` flag tells the query layer
-- which strategy to use. This avoids storing duplicate rows for symmetric relationships.

-- Full-text search
CREATE VIRTUAL TABLE search_index USING fts5(
    entity_id, entity_type, content,
    tokenize='porter unicode61'
);

-- Audit log (append-only)
-- Stores field-level diffs, not full entity snapshots. For 'create', old_value is NULL and
-- new_value contains the full entity. For 'update', both contain only the changed fields
-- (JSON patch format: {"field": {"old": ..., "new": ...}}). For 'delete', new_value is NULL
-- and old_value contains the full entity (enabling undo). This keeps the audit log compact
-- for large entities where only one field changed.
CREATE TABLE audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,         -- ISO 8601 UTC
    actor TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    action TEXT NOT NULL,          -- 'create', 'update', 'delete'
    old_value JSON,               -- NULL for 'create'; changed fields for 'update'; full entity for 'delete'
    new_value JSON                -- full entity for 'create'; changed fields for 'update'; NULL for 'delete'
);

-- Event log for agent replay (retained 30 days, pruned on startup)
CREATE TABLE event_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,         -- ISO 8601 UTC
    event_type TEXT NOT NULL,        -- 'entity.created', 'media.uploaded', etc.
    entity_id TEXT,
    entity_type TEXT,
    payload JSON NOT NULL,           -- full event payload
    expires_at TEXT NOT NULL         -- ISO 8601 UTC, timestamp + 30 days
);
CREATE INDEX idx_event_log_type_time ON event_log(event_type, timestamp);

-- Research log (GPS element 1: reasonably exhaustive search)
CREATE TABLE research_log (
    id TEXT PRIMARY KEY,
    date TEXT NOT NULL,               -- ISO 8601 UTC
    objective TEXT NOT NULL,
    repository_id TEXT,               -- FK to repositories (nullable)
    repository_name TEXT,             -- free text fallback
    search_terms JSON NOT NULL,       -- array of strings
    source_id TEXT,                   -- FK to sources (nullable)
    result TEXT NOT NULL,             -- 'found', 'not_found', 'partially_found', 'inconclusive'
    findings TEXT,
    citations_created JSON,           -- array of citation UUIDs
    next_steps TEXT,
    person_refs JSON,                 -- array of person UUIDs
    tags JSON                         -- array of strings
);
CREATE INDEX idx_research_log_date ON research_log(date);
CREATE INDEX idx_research_log_result ON research_log(result);

-- DNA data (Phase 4+)
CREATE TABLE dna_tests (
    id TEXT PRIMARY KEY,
    person_id TEXT NOT NULL,          -- FK to persons
    test_type TEXT NOT NULL,          -- 'autosomal', 'ydna', 'mtdna', 'xdna'
    provider TEXT NOT NULL,
    kit_id TEXT,
    haplogroups JSON,
    ethnicity_estimate JSON,
    import_date TEXT NOT NULL
);

CREATE TABLE dna_matches (
    id TEXT PRIMARY KEY,
    test_id TEXT NOT NULL REFERENCES dna_tests(id),
    matched_person_id TEXT,           -- FK to persons (nullable until identified)
    matched_name TEXT NOT NULL,
    shared_cm REAL NOT NULL,
    shared_segments INTEGER NOT NULL,
    longest_segment_cm REAL,
    estimated_relationship TEXT,
    segments JSON                     -- array of DnaSegment objects
);
CREATE INDEX idx_dna_matches_cm ON dna_matches(shared_cm DESC);

-- Agent registry
CREATE TABLE agents (
    id TEXT PRIMARY KEY,             -- agent name
    api_key_hash TEXT NOT NULL,      -- bcrypt/argon2 hash of the API key
    registered_at TEXT NOT NULL,     -- ISO 8601 UTC
    last_seen_at TEXT,               -- ISO 8601 UTC, updated on each API call
    status TEXT NOT NULL DEFAULT 'active',  -- 'active', 'inactive', 'error'
    config JSON                      -- agent-specific configuration
);
```

##### 4.3 Media Storage

Media files stored on the local filesystem alongside the SQLite database:

```
~/.rustygene/                    # or user-configured location
├── rustygene.db                 # SQLite database
├── media/
│   ├── files/
│   │   ├── {content-hash}.jpg   # content-addressed originals
│   │   └── ...
│   └── thumbs/
│       ├── {content-hash}.jpg   # generated thumbnails
│       └── ...
└── backups/                     # automated backup copies
```

Media metadata (hash, mime type, OCR text, dimensions in pixels, physical dimensions in millimetres where known) stored in SQLite. All physical measurements are metric-only — never store or display imperial units. Files content-addressed by hash for deduplication. Multiple metadata records can reference the same file hash (same image, different captions/contexts).

**Supported media types:**
* **Documents:** Birth/death/marriage certificates, census pages, wills, probate records, parish register entries (JPEG, PNG, TIFF, PDF).
* **Photographs:** Family photos, headstone photos, house photos (JPEG, PNG, HEIC).
* **Audio/Video:** Oral history recordings, interviews (MP3, MP4, WAV) — metadata only, no transcription in Phase 1.
* **Transcriptions:** Manual or OCR-generated text associated with a document image.

Each media item can be linked to one or more entities (a census page image links to multiple Persons via their Citation).

##### 4.4 Export for Portability & Git

While SQLite is the live database, full JSON export is available for portability:
* `rustygene export --format json` dumps the entire database to a directory of JSON files (one per entity type or per entity).
* `rustygene export --format gedcom` produces GEDCOM 5.5.1.
* `rustygene export --format bundle` produces JSON + media files as a zip.
* These exports are human-readable, diffable, and can be committed to git for versioning/backup.
* `rustygene import` can rebuild the database from any export.

##### 4.5 Trait Abstraction (Future Backends)

The storage layer is behind a `Storage` trait in `crates/storage/`. SQLite is the only implementation initially.

Future backends (added without touching `core`, `api`, or frontend):
* **PostgreSQL + Apache AGE** — for a multi-user server edition. openCypher graph queries, `tsvector` search, S3 media storage.
* **Cloud-sync** — SQLite locally + sync to a remote store (Turso/libSQL, or custom sync protocol).

**SurrealDB is excluded.** v3.0 (2026-02) has open data-correctness bugs. Unacceptable for genealogy data where integrity is paramount.

##### 4.6 Key Capabilities

* **Graph traversal:** Recursive CTEs over the `relationships` table. Performant for ancestor/descendant chains at this scale.
* **Search (multi-strategy):** Name search is hard in genealogy — spelling varies wildly across records (Smith/Smyth/Smythe, Elisabeth/Elizabeth, Wm/William). The search system supports multiple strategies:
    * **Exact match:** FTS5 standard query.
    * **Stem matching:** FTS5 porter tokenizer handles common stems (e.g., "running" matches "run").
    * **Phonetic matching:** Soundex/Metaphone/Double Metaphone for name variants that sound alike (Smith ↔ Smyth). Stored as additional indexed columns.
    * **Known alternates:** A configurable name-variants table mapping common alternate names (William ↔ Wm ↔ Will ↔ Bill, Margaret ↔ Peggy ↔ Maggie, Elisabeth ↔ Elizabeth).
    * **Fuzzy/typo tolerance:** Levenshtein distance or trigram matching (`pg_trgm` in PG, application-level in SQLite) for OCR errors and transcription mistakes.
    * **Date range search:** "born about 1850" searches 1848-1852. Tolerance configurable per query.
    * **Combined:** Search "William Smith born ~1850 Yorkshire" uses all strategies simultaneously, ranked by relevance (exact > phonetic > fuzzy).
    * **Geographic resolution weighting:** Not all location assertions carry equal weight. A self-reported, highly specific birthplace (e.g., "Llanpumsaint" from an 1901 census where the person stated it themselves) is a **high-resolution anchor** — it should be treated with much higher confidence than a county-level location ("Carmarthenshire") from a secondary source. The search and deduplication systems should:
        * Assign a `geographic_specificity` score based on place type (Village > Town > County > Country).
        * Heavily penalise merge proposals that try to combine a high-resolution person with a low-resolution person from a different specific location, unless the timeline plausibly explains migration.
        * Prefer high-specificity self-reported locations (census birthplace fields) over locations inferred from record jurisdiction.
* **Core validation rules:** The Rust storage layer enforces basic sanity checks on every mutation, independent of AI agents:
    * Birth date must precede death date.
    * Parent must be older than child (minimum age gap configurable, default 12 years).
    * No impossible dates (31 Feb, 31 Apr, etc.).
    * Event dates must fall within the person's lifespan (with tolerance for imprecise dates).
    * Relationship constraints: a person cannot be their own parent/child.
    * These are hard constraints — rejected at the API layer with a 422 error and a clear message. Agents proposing data that violates these constraints have their proposals auto-rejected.
* **Audit log:** Append-only `audit_log` table. Every mutation records timestamp, actor, entity, old/new value. Enables undo.
* **Backup/restore:** Copy the SQLite file + media directory. Application-level: GEDCOM or JSON bundle export.
* **Versioning:** Assertions are never deleted, only superseded (status changes from `confirmed` to `rejected`, new assertion added). Audit log enables viewing state at any past point.

---

#### 5. Event Bus & Agent Integration

The event bus decouples the core from consumers. It is the mechanism by which agents (and the UI) learn about changes.

##### 5.1 Event Types

```
entity.created    — a new Person/Family/Event/etc. was added
entity.updated    — an entity's assertions changed
entity.deleted    — an entity was removed
media.uploaded    — a new file was attached
media.extracted   — OCR/vision extraction completed
proposal.submitted — an agent submitted a new proposal
proposal.reviewed  — a human approved/rejected a proposal
import.completed   — a GEDCOM/CSV import finished
```

##### 5.2 Delivery Mechanisms

* **Internal (Tauri):** In-process Rust channels. UI subscribes to update views reactively.
* **External (Agents):**
    * **SSE (Server-Sent Events):** `GET /events/stream?types=entity.created,media.uploaded`. The core pushes events over a persistent HTTP stream. Agents open a connection and react to JSON payloads. Preferred over webhooks — agents don't need to run their own HTTP server, avoiding port conflicts on desktop.
    * **Polling:** `GET /events?since={timestamp}&types=entity.created,media.uploaded`. Fallback for agents that cannot maintain a persistent connection.
    * **Future:** WebSockets or message queue (NATS, Redis streams) if bidirectional communication or throughput demands it. Not needed initially.

##### 5.3 Agent Protocol

An agent is anything that:
1. Authenticates (API key or local socket).
2. Reads from `/graph` and `/search`.
3. Subscribes to events (SSE stream or polling).
4. Writes proposals to `/staging` with confidence score and source citations.
5. **Never** writes directly to `/graph/mutate`.

Agents can be written in any language, run anywhere, and be swapped/added/removed without touching the core. The OpenAPI spec is the contract.

##### 5.4 Event Replay

Agents that disconnect can replay missed events via `GET /events?since={timestamp}`. The `event_log` table retains events for 30 days (pruned on startup). This ensures agents recover cleanly after restarts or crashes.

##### 5.5 Agent Infrastructure (Shared)

When building multiple agents, common infrastructure should be extracted:
* **Shared client library:** Auto-generated from OpenAPI spec. Handles auth, retries, rate limiting.
* **Agent base class / trait:** Common event subscription, health reporting, logging, configuration.
* **Agent registry:** The core tracks registered agents in the `agents` table (`GET /agents`). Agents report health via periodic heartbeats. UI shows agent status. Agents not seen for a configurable timeout (default 5 minutes) are marked `inactive`.
* **Shared tool definitions:** Connectors (FamilySearch, Discovery API) are Rust library crates exposed as API endpoints. Agents call them via REST, not by reimplementing HTTP clients.

##### 5.6 Agent Failure Handling

* **Proposal validation:** The `/staging` endpoint validates all proposals before storage. Malformed JSON, missing required fields, or invalid entity references are rejected with a 422 response. Agents never corrupt the staging queue.
* **Health monitoring:** `GET /agents/{id}/health` returns last heartbeat, error count, and last error message. UI displays agent health dashboard.
* **Crash recovery:** Agents are stateless consumers of the event stream. On restart, they resume from the last processed event timestamp (stored locally by the agent). No server-side state to recover.
* **External API failures:** Agent base class implements exponential backoff with jitter for external API calls (FamilySearch, Discovery API, LLM providers). Circuit breaker pattern after N consecutive failures.

---

#### 6. REST API (Axum + utoipa)

##### REST vs GraphQL

**REST chosen for Phase 1-3.** Reasons:
* `utoipa` auto-generates OpenAPI specs from Axum handlers — single source of truth, auto-generated client SDKs.
* Agents are simple consumers — they don't need the query flexibility of GraphQL.
* Caching, ETags, and optimistic locking map naturally to HTTP semantics.
* GraphQL adds complexity (schema stitching, N+1 queries, batching) without clear benefit at this scale.

**GraphQL may be added later** as an alternative API layer (Phase 5+) for the desktop/web UI where flexible queries over the graph would reduce round-trips. `async-graphql` is the mature Rust GraphQL library. The trait-based storage abstraction supports both REST and GraphQL resolvers without duplication.

##### Resource-Oriented API Design

All mutations require `If-Match: <version>` header for optimistic locking. Version mismatch returns `409 Conflict`.

**Entity resources** (standard CRUD pattern for each entity type):

| Resource | Methods | Notes |
|---|---|---|
| `/api/v1/persons` | `GET` (list, filter, paginate), `POST` (create) | |
| `/api/v1/persons/{id}` | `GET`, `PUT`, `DELETE` | Includes assertions, names, linked events |
| `/api/v1/persons/{id}/assertions` | `GET`, `POST` | All assertions for this person |
| `/api/v1/persons/{id}/relationships` | `GET` | All relationships involving this person |
| `/api/v1/persons/{id}/media` | `GET` | Linked media items |
| `/api/v1/persons/{id}/timeline` | `GET` | Chronological events for this person |
| `/api/v1/families` | `GET`, `POST` | |
| `/api/v1/families/{id}` | `GET`, `PUT`, `DELETE` | |
| `/api/v1/relationships` | `GET`, `POST` | Pairwise relationships (couple, parent-child, etc.) |
| `/api/v1/relationships/{id}` | `GET`, `PUT`, `DELETE` | |
| `/api/v1/events` | `GET`, `POST` | |
| `/api/v1/events/{id}` | `GET`, `PUT`, `DELETE` | Includes participants with roles |
| `/api/v1/places` | `GET`, `POST` | |
| `/api/v1/places/{id}` | `GET`, `PUT`, `DELETE` | Includes hierarchy (enclosed-by) |
| `/api/v1/sources` | `GET`, `POST` | |
| `/api/v1/sources/{id}` | `GET`, `PUT`, `DELETE` | |
| `/api/v1/citations` | `GET`, `POST` | |
| `/api/v1/citations/{id}` | `GET`, `PUT`, `DELETE` | |
| `/api/v1/repositories` | `GET`, `POST` | |
| `/api/v1/repositories/{id}` | `GET`, `PUT`, `DELETE` | |
| `/api/v1/notes` | `GET`, `POST` | |
| `/api/v1/notes/{id}` | `GET`, `PUT`, `DELETE` | |

**Media resources:**

| Resource | Methods | Notes |
|---|---|---|
| `/api/v1/media` | `GET` (list), `POST` (upload, multipart) | |
| `/api/v1/media/{id}` | `GET` (metadata), `DELETE` | |
| `/api/v1/media/{id}/file` | `GET` (download original) | |
| `/api/v1/media/{id}/thumbnail` | `GET` (download thumbnail) | |
| `/api/v1/media/{id}/extract` | `POST` (trigger OCR/vision) | |

**Staging & review:**

| Resource | Methods | Notes |
|---|---|---|
| `/api/v1/staging` | `GET` (list proposals), `POST` (submit proposal) | Deduplicated via idempotency key. |
| `/api/v1/staging/{id}` | `GET`, `DELETE` | |
| `/api/v1/staging/{id}/approve` | `POST` | Move to confirmed assertions. |
| `/api/v1/staging/{id}/reject` | `POST` | Mark as rejected with reason. |
| `/api/v1/staging/bulk` | `POST` | Bulk approve/reject. Body: `{ids: [...], action: "approve"|"reject"}` |

**Search:**

| Resource | Methods | Notes |
|---|---|---|
| `/api/v1/search` | `GET` | `?q=...&type=person&strategy=phonetic&date_range=1850..1870&place=yorkshire` |

**Graph traversal:**

| Resource | Methods | Notes |
|---|---|---|
| `/api/v1/graph/ancestors/{person_id}` | `GET` | `?generations=5`. Returns ancestor tree. |
| `/api/v1/graph/descendants/{person_id}` | `GET` | `?generations=5`. Returns descendant tree. |
| `/api/v1/graph/path/{person_id_1}/{person_id_2}` | `GET` | Shortest relationship path between two persons. |
| `/api/v1/graph/pedigree/{person_id}` | `GET` | Pedigree data (optimised for chart rendering). |

**Import/export:**

| Resource | Methods | Notes |
|---|---|---|
| `/api/v1/import` | `POST` | Multipart upload. `?format=gedcom|csv|gramps_xml`. Returns job ID. |
| `/api/v1/import/{job_id}` | `GET` | Import job status and report. |
| `/api/v1/export` | `GET` | `?format=gedcom|gedcom7|json|bundle`. Streams the export. |

**Research log:**

| Resource | Methods | Notes |
|---|---|---|
| `/api/v1/research-log` | `GET` (list, filter by person/result/date), `POST` (create entry) | |
| `/api/v1/research-log/{id}` | `GET`, `PUT`, `DELETE` | |

**DNA (Phase 4+):**

| Resource | Methods | Notes |
|---|---|---|
| `/api/v1/dna/tests` | `GET`, `POST` | |
| `/api/v1/dna/tests/{id}` | `GET`, `PUT`, `DELETE` | |
| `/api/v1/dna/tests/{id}/matches` | `GET` | Matches for a specific test. `?min_cm=20&sort=shared_cm` |
| `/api/v1/dna/matches/{id}` | `GET`, `PUT` | Link match to local person, update details. |

**Infrastructure:**

| Resource | Methods | Notes |
|---|---|---|
| `/api/v1/events` | `GET` | Polling. `?since={timestamp}&types=entity.created,...` |
| `/api/v1/events/stream` | `GET` | SSE stream for real-time delivery. |
| `/api/v1/agents` | `GET` (list), `POST` (register) | |
| `/api/v1/agents/{id}` | `GET`, `DELETE` | Includes health, last seen, error count. |
| `/api/v1/health` | `GET` | System health, queue depth, storage stats. |

All endpoints versioned under `/api/v1/`. OpenAPI spec auto-generated via `utoipa`. Client SDKs for Python (and any other language) derived from the spec.

---

#### 7. Import / Export

| Format | Direction | Phase | Notes |
|---|---|---|---|
| GEDCOM 5.5.1 | Import + Export | 1A | Semantic fidelity for core records. Unknown tags preserved via `_raw_gedcom`. See §7.1. |
| JSON | Import + Export | 1A | Native format. Full fidelity including assertions, confidence scores, audit history. |
| Gramps XML | Import | 1B | Many users migrating from Gramps. Parse with `quick-xml`. Critical migration path. |
| CSV | Import | 1B | Bulk import of transcribed records (census, BMD indexes). |
| GEDCOM 7.0 | Export (import later) | 3+ | Newer standard, low adoption so far. Build on domain model. |
| Media bundle | Export | 2 | Zip of all attached files + manifest JSON. |

##### 7.1 GEDCOM Round-Trip Fidelity Tiers

See also the phased fidelity note in §3.1 Design Notes. The tiers are:

1. **Semantic fidelity** (Phase 1A): Import → export → re-import produces an identical assertion graph. Field values, relationships, citations, and entity structure are preserved. Formatting, tag ordering, and whitespace may differ.
2. **Tag coverage** (Phase 1B): All standard GEDCOM 5.5.1 tags are handled (not just the common subset). Corpus testing against real-world files from major vendors.
3. **Textual fidelity** (Phase 3+): Minimise diff between input and output. Preserve ordering and formatting where feasible. "Any delta is a bug" as the aspirational bar.

##### 7.2 GEDCOM Merge Import (Diff / Selective Import)

Phase 1A import assumes an empty database — every record is new. Real-world usage quickly demands importing a GEDCOM file *into an existing database*: merging a cousin's export with your own tree, re-importing an updated Ancestry export, or comparing two independently researched branches of the same family.

**Three import modes:**

| Mode | Behaviour | Phase |
|---|---|---|
| `fresh` | Current behaviour — create assertions for every record. Fails if the database already contains entities (unless `--force` overrides). | 1A |
| `diff` | Parse the GEDCOM, match entities against the existing graph, produce a read-only comparison report. No writes. | 1B |
| `merge` | Parse, match, then submit differing/new assertions to the staging queue as proposals for human review. | 1B (deterministic matching), 3+ (agent-assisted matching) |

**Entity matching engine:**

The matcher is a standalone module in `crates/gedcom` (or a shared `crates/matching` crate) reused by the dedup agent (§5) and the merge import pipeline. It produces candidate matches with confidence scores:

```
GedcomPerson(incoming) × Person(existing) → Vec<CandidateMatch { existing_id, score, evidence }>
```

Matching signals, combined via weighted scoring:

| Signal | Weight | Notes |
|---|---|---|
| Name similarity | High | Exact match, phonetic (Soundex/Double Metaphone), known alternates (William ↔ Wm), surname-origin-aware (patronymic *ap* prefixes) |
| Birth date overlap | High | Fuzzy tolerance via `DateValue` — "about 1850" overlaps 1848–1852 |
| Death date overlap | Medium | Same fuzzy logic |
| Birth/death place | Medium | Hierarchical place matching — "Llanpumsaint, Carmarthenshire" matches "Llanpumsaint" with higher confidence than "Carmarthenshire" alone. Geographic specificity weighting per §4.6. |
| Family structure | Medium | Shared parent/child/spouse names strengthen a match. Two persons with the same name + same father's name + same birth year are almost certainly the same person. |
| Gender | Low (filter) | Mismatch eliminates candidate unless gender is unknown on either side |
| External IDs | Definitive | Matching `_UID`, REFN, or other external identifiers → automatic match |

**Score thresholds (configurable):**
* `>= 0.95` — **Auto-match.** High confidence. Shown as matched in the report, linked automatically in merge mode (user can still override).
* `0.70 – 0.94` — **Candidate match.** Requires human confirmation. Presented with a side-by-side comparison.
* `< 0.70` — **No match.** Treated as a new entity.

**Diff report structure:**

The diff report groups results into four categories:

1. **Matched — identical.** Incoming entity matched an existing entity, and all assertions are equivalent. No action needed. Count only.
2. **Matched — differences found.** Incoming entity matched, but some assertions differ (new facts, conflicting values, additional sources). Listed per-assertion with a side-by-side comparison: `existing value | incoming value | conflict type (new/changed/additional_source)`.
3. **Incoming only.** Entity exists in the GEDCOM but has no match in the database. New person/family/event/source.
4. **Existing only.** Entity exists in the database but has no match in the GEDCOM. Informational — the merge import never deletes existing data.

**Merge mode workflow:**

1. **Parse.** GEDCOM → intermediate representation (same as fresh import).
2. **Match.** Run the matching engine. Auto-matches are linked. Candidate matches are flagged for review.
3. **Human review (candidate matches).** For each candidate, the user chooses: *match* (link entities), *not a match* (treat incoming as new), or *skip* (exclude from this import entirely).
4. **Diff.** For all matched pairs, compute per-assertion diffs.
5. **Select.** The user reviews diffs and selects what to import. Granularity options:
   * **Per-assertion** (default): Cherry-pick individual facts. "Accept incoming birth date, keep existing death date, take the new census event."
   * **Per-entity**: Accept or reject all incoming assertions for a matched entity.
   * **Bulk**: "Import all new entities", "Import all new assertions for matched entities", "Import nothing — report only."
6. **Submit.** Selected assertions enter the staging queue as proposals with `proposed_by: import:<job_id>`. Each proposal carries a reference to the GEDCOM source (file path + line number) as provenance.
7. **Review.** Normal staging queue review workflow (approve/reject per proposal or bulk). This is the same flow used for agent proposals — no new UI needed beyond the import-specific match/diff views.

**Conflict handling:**

When an incoming assertion conflicts with an existing `Confirmed` assertion for the same entity and field:

* The incoming value enters the staging queue as a `Proposed` assertion — it does not overwrite.
* The diff report highlights the conflict with both values.
* If the user approves the incoming assertion, it becomes a second `Confirmed` assertion (competing assertions coexist per §3.2). The user can then mark one as `preferred`.
* If the incoming assertion carries a source citation not present on the existing assertion, it can be added as additional evidence for the existing assertion rather than creating a competing one. The UI offers this as an explicit choice: "Add as new assertion" vs "Add source to existing assertion."

**CLI:**

```
rustygene import <file.ged>                          # fresh import (Phase 1A, existing behaviour)
rustygene import <file.ged> --mode diff              # diff report only, no writes
rustygene import <file.ged> --mode diff --format json # machine-readable diff report
rustygene import <file.ged> --mode merge             # interactive merge import
rustygene import <file.ged> --mode merge --auto-accept-threshold 0.95  # override auto-match threshold
rustygene import <file.ged> --mode merge --bulk-new  # auto-queue all unmatched entities (skip per-entity review)
```

**API:**

| Resource | Methods | Notes |
|---|---|---|
| `/api/v1/import` | `POST` | `?mode=fresh|diff|merge`. Default: `fresh` if database empty, `diff` if non-empty. Returns job ID. |
| `/api/v1/import/{job_id}` | `GET` | Job status, progress, and diff report (when complete). |
| `/api/v1/import/{job_id}/matches` | `GET` | List candidate matches for human review. |
| `/api/v1/import/{job_id}/matches/{match_id}` | `PUT` | Confirm or reject a candidate match. Body: `{decision: "match"|"not_match"|"skip"}` |
| `/api/v1/import/{job_id}/diffs` | `GET` | Per-assertion diffs for all matched entities. Filterable by entity, conflict type. |
| `/api/v1/import/{job_id}/submit` | `POST` | Submit selected assertions to staging queue. Body: `{selections: [...], bulk_strategy: "all_new"|"all_diffs"|"selected"}` |

**Phase notes:**
* **Phase 1B:** Deterministic matching engine (name + date + place + family structure). CLI `diff` and `merge` modes. Report output. Staging queue submission (internal, no API). This is the workhorse — covers 80% of real-world merge scenarios.
* **Phase 3:** REST API endpoints above. Desktop UI: import wizard with side-by-side entity comparison, per-assertion checkboxes, bulk actions. Integration with staging queue review dashboard.
* **Phase 4–5:** Agent-assisted matching. The dedup agent (§5, Phase 5) can be invoked to score ambiguous candidate matches using LLM reasoning over the full evidence context. The matching engine calls out to the agent for candidates in the 0.50–0.70 range that deterministic scoring cannot resolve.

---

#### 8. Document Processing & Image Recognition

A core capability, not an agent. Exposed as an API endpoint and also usable by agents.

**Pipeline:**
```
Image/PDF upload → media storage → OCR/vision extraction → structured assertions → staging queue
```

* LLM vision API (Gemini, Claude, or local model) for handwritten record OCR — census entries, parish registers, certificates, wills.
* Returns structured JSON: names, dates, places, relationships extracted from the document.
* All extractions land in the staging queue as `Proposed` assertions with the media item as source citation.
* User reviews, corrects, confirms.

**Endpoint:** `POST /media/{id}/extract`

This is the killer feature for genealogy research — old handwriting is the #1 bottleneck.

---

#### 9. External Data Source Connectors

##### 9.1 Connector Architecture

Connectors are **pluggable adapters** for external genealogy data providers. Each connector implements a common `Connector` trait, is packaged as a separate Rust crate (or Python package for API-only connectors), and is registered at runtime. Adding a new data source means writing one crate — no changes to core, API, or UI.

```rust
/// Every connector implements this trait. The core doesn't know or care
/// which provider is behind it — it just gets search results and records.
#[async_trait]
trait Connector: Send + Sync {
    /// Human-readable name ("FamilySearch", "FindMyPast", ...).
    fn name(&self) -> &str;

    /// What this connector can do. Drives UI display and agent decisions.
    fn capabilities(&self) -> ConnectorCapabilities;

    /// Search the external provider. Returns normalized results.
    async fn search(&self, query: &SearchQuery) -> Result<Vec<ExternalRecord>>;

    /// Fetch a single record by its provider-specific ID.
    async fn get_record(&self, external_id: &str) -> Result<ExternalRecord>;

    /// Check whether credentials are configured and valid.
    async fn health_check(&self) -> Result<ConnectorHealth>;
}

struct ConnectorCapabilities {
    can_search_persons: bool,
    can_search_records: bool,
    can_fetch_images: bool,
    can_search_places: bool,
    requires_auth: bool,
    rate_limit: Option<RateLimit>,   // requests per second/minute
    delay_between_requests_ms: Option<u64>, // crawl delay — volunteer sites and gov APIs
                                            // aggressively block IPs that don't respect this
    supported_regions: Vec<String>,  // geographic focus (if any)
}

/// Normalized record from any provider. Provider-specific details in `raw_data`.
struct ExternalRecord {
    provider: String,
    external_id: String,            // provider-specific ID
    record_type: ExternalRecordType, // Census, BirthReg, DeathReg, Marriage, Baptism, Burial, ...
    title: String,
    persons: Vec<ExtractedPerson>,  // persons mentioned in this record
    date: Option<DateValue>,
    place: Option<String>,
    source_citation: String,        // human-readable citation for this record
    image_url: Option<String>,      // link to document image (if available)
    raw_data: serde_json::Value,    // full provider response for lossless storage
}
```

##### 9.2 Connector Registry & Configuration

Connectors are registered in a `connectors.toml` configuration file:

```toml
[familysearch]
enabled = true
crate = "rustygene-connector-familysearch"
auth_type = "oauth2"
# Credentials stored separately in OS keychain or env vars, never in config.

[findmypast]
enabled = false
crate = "rustygene-connector-findmypast"
auth_type = "api_key"

[wikitree]
enabled = true
crate = "rustygene-connector-wikitree"
auth_type = "none"   # public read API
```

The connector registry is exposed via the REST API (`GET /api/v1/connectors`) so the UI can show which providers are available and their health status. Agents query the registry to discover which connectors are active.

##### 9.3 REST API for Connectors

| Resource | Methods | Notes |
|---|---|---|
| `/api/v1/connectors` | `GET` | List registered connectors with status and capabilities. |
| `/api/v1/connectors/{name}/search` | `GET` | Search a specific provider. `?q=...&type=person&date_range=...` |
| `/api/v1/connectors/{name}/records/{external_id}` | `GET` | Fetch a specific record from the provider. |
| `/api/v1/connectors/{name}/health` | `GET` | Auth status, rate limit remaining, last error. |
| `/api/v1/connectors/search` | `GET` | **Federated search** across all enabled connectors. Results tagged by provider. |

##### 9.4 Known Providers

| Provider | Auth | API Type | Coverage | Notes |
|---|---|---|---|---|
| **FamilySearch** | OAuth 2.0 (Solutions Provider) | REST (GEDCOM X) | Global, largest free collection | Read-only recommended. Collaborative tree — writes need conflict handling. `gedcomx` crate for types. |
| **Ancestry** | No public API | — | Global, largest paid collection | No connector possible without partnership. Import via GEDCOM export only. |
| **FindMyPast** | API key (partnership) | REST | UK/Ireland, strong BMD + census | Commercial — requires data partnership agreement. |
| **MyHeritage** | OAuth 2.0 | REST | Global, strong European coverage | API available for registered apps. DNA matching integration. |
| **WikiTree** | None (public read) | REST | Global, collaborative | Free, open data. Good for cross-tree linking. |
| **UK National Archives (Discovery)** | None | REST | UK government archives | Free search. Document images via commercial partners. |
| **ScotlandsPeople** | No public API | — | Scotland BMD, census, wills | Manual export only. CSV importer for purchased data. |
| **FreeCEN / FreeBMD / FreeREG** | None | Bulk download | UK census, BMD, parish registers | **Do not scrape** — volunteer-run. Contact FreeUKGen about data access. CSV importers for bulk data. |
| **GeneaNet** | No public API | — | France, strong European coverage | GEDCOM import/export only. |
| **Billiongraves / FindAGrave** | Varies | REST / scrape risk | Cemetery records | BillionGraves has API; FindAGrave does not (Ancestry-owned). |
| **GeoNames** | None | REST | Place authority (12M names) | For place hierarchy enrichment, not person records. |
| **Getty TGN** | None | SPARQL / bulk | Historical place names (2.4M) | Polyhierarchy, temporal scope. Phase 3+. |

##### 9.5 Adding a New Connector

1. Create a new crate in `crates/connectors/` (or a Python package in `agents/packages/` for API-only connectors).
2. Implement the `Connector` trait (Rust) or the equivalent Python protocol.
3. Add an entry to `connectors.toml`.
4. Restart the application. The connector is auto-discovered and registered.

No changes to core, storage, API, or UI. The federated search endpoint and agent infrastructure automatically pick up the new provider.

---

#### 10. User Interfaces

Three interface layers, all backed by the same Rust core and REST API:

##### 10.1 CLI (`rustygene`)

The primary interface for Phase 1 development and power users. Built with `clap`.

```
rustygene import <file.ged>           # GEDCOM import (fresh or auto-detect)
rustygene import <file.ged> --mode diff   # diff report against existing data
rustygene import <file.ged> --mode merge  # interactive selective merge (§7.2)
rustygene export --format gedcom      # GEDCOM export
rustygene export --format json        # JSON export
rustygene query persons --name "Smith" --birth-range 1850..1870
rustygene show person <uuid>          # detailed person view
rustygene search "census 1881 yorkshire"
rustygene agent register <name>       # register an agent
rustygene agent list                  # show agent status
rustygene backup                      # copy database + media
rustygene restore <backup-path>
```

##### 10.2 TUI (Terminal UI — Optional)

An interactive terminal interface inspired by k9s, lazygit, claude. Built with `ratatui`. Provides:
* Navigable person/family/event lists with vim-style keybindings
* Inline pedigree rendering (ASCII art tree)
* Review queue with approve/reject workflow
* Search with live results
* Agent status dashboard

Not a Phase 1 priority, but a natural extension of the CLI for users who prefer terminal workflows.

##### 10.3 Desktop App (Tauri 2.x + Svelte 5)

A fast, local-first, offline-capable desktop application.

**Technology choices:**
* **Svelte 5** over React: smaller bundles (~47KB vs ~156KB), lower memory, surgical DOM updates via runes. Graph viz libraries (Cytoscape, D3) have vanilla JS APIs — no need for framework-specific wrappers.
* **Cytoscape.js 3.33:** Primary relationship graph view. Handles arbitrary topologies (pedigree collapse, multiple families). Performant at 1000+ nodes. **IPC consideration:** Passing 5000+ JSON node objects through Tauri IPC can cause UI stutter. The `/graph/pedigree/{id}` endpoint must support a `?generations=N` viewport radius parameter (default 3-4). Load immediate generations first; fetch further branches asynchronously as the user pans the canvas.
* **D3.js:** Specialized views — fan charts (radial ancestor view), pedigree charts. Reference implementation: Gramps Web's D3 chart code.

**Visual language:**
* Solid lines/borders = Confirmed assertions.
* Dashed lines/borders = Proposed (unreviewed) assertions.
* Colour gradients (red → amber → green) on nodes/edges mapping to `confidence` score.
* Disputed assertions highlighted with a distinct indicator.

**Key views:**
* **Pedigree chart:** Traditional ancestor tree.
* **Descendant chart:** Top-down from an ancestor.
* **Fan chart:** Radial ancestor view (D3).
* **Relationship graph:** Full network view (Cytoscape). The primary differentiator.
* **Person detail:** Timeline of events, attached media, source citations, competing assertions.
* **Timeline view:** Chronological timeline of all events for a person, family, or the entire tree. Zoomable.
* **Map view:** Events plotted on a geographic map (migration paths, census locations, birth/death places). Leaflet or MapLibre. Shows movement over time.
* **Document viewer:** Image + OCR text side-by-side, annotation tools.
* **Review queue:** Bulk approve/reject/modify agent proposals.
* **Search:** Full-text across all entities, notes, OCR text.
* **Reports:** Printable/PDF pedigree charts, family group sheets, ahnentafel reports, descendant reports. Standard genealogy outputs.

**Additional key views (informed by most-requested Gramps addons — these should be native, not plugins):**
* **Chromosome painter:** Visual chromosome map of DNA segment matches, painted across all 22 autosomes + X. Clickable segments link to matched persons. (Phase 4+, requires DNA data model.)
* **Geographic heatmap:** Density overlay on the map view showing concentration of family events by region/era. Reveals migration patterns at a glance.
* **Lifeline chart:** Horizontal lifeline bars on a common time axis. Instantly shows who was alive simultaneously, generational overlap, and age-at-event. (D3.js.)
* **Shareable web export:** Generate a self-contained static HTML/JS family tree (no server needed) for sharing with non-technical family members. D3 interactive charts + narrative pages. Export via CLI (`rustygene export --format web`) or UI.

**Source-driven data entry:** Form-based entry mode that mirrors real-world documents — birth certificates, census forms, parish register entries, marriage records. The user fills in fields as they appear on the source document; the system maps them to the correct entities, events, and citations. This is the natural way genealogists work (source → data, not data → source). Configurable form templates for common document types per country/era.

**Estimated date calculation:** Heuristic engine that estimates missing birth/death dates from known dates of relatives (parents' births, children's births, marriage dates, census ages). Configurable rules (e.g., "if no birth date, estimate from first child's birth minus 25 years"). Estimates are stored as `About` DateValues with low confidence, clearly marked in the UI. Enables timeline and chart views to function with incomplete data.

**Session restore:** The app remembers the last active view, selected person, scroll position, and open panels. Restart picks up exactly where you left off. Maintains a recent-items history (last 20 visited persons/families/events) accessible via a Go menu or `Ctrl-R`.

**Bulk operations:**
* Import CSV of census records, bulk approve/reject proposals, bulk mark persons as confirmed.
* **Bulk privacy management:** Set/clear privacy flags on all persons matching a filter (e.g., "born within last 100 years"). Essential before any export or sharing.
* **Bulk tagging:** Add/remove tags on filtered sets of any entity type.
* **Bulk source attachment:** Attach a single source/citation to all persons from a filtered set (e.g., all persons on a census page).
* **Batch find-and-replace** on event descriptions, place names, notes.

**Media gallery and organisation:** Content-addressed storage (§4.3) handles deduplication on disk, but users need logical organisation to avoid an unsearchable grid of hash-named files. The media UI provides:
* **Virtual albums/collections:** User-created groupings (e.g., "1881 Census Pages", "Jones Family Photos", "Headstones — Llanpumsaint"). A media item can belong to multiple albums. Albums are metadata, not filesystem copies.
* **Custom tags:** Free-text tags on any media item, filterable and searchable. Tags are shared across the media library (typeahead from existing tags).
* **Structured captions:** Each media item carries a title, description, date (imprecise — reuses `DateValue`), and place ref. Captions are searchable via FTS5.
* **Sort and filter:** Sort by date, entity link count, album, tag, upload date, file type. Filter by unlinked (orphaned), untagged, or specific entity associations.
* **Thumbnail grid + list view:** Toggle between visual grid (thumbnails) and tabular list (metadata columns). Grid view shows caption on hover.
* **Automatic entity linking:** When a media item is attached to an Event, all Event participants are suggested as entity links. Bulk-accept or pick individually.

**Data quality tools (native, not plugins):**
* **Media integrity checker:** Verify all media references resolve to files on disk. Flag missing, moved, or orphaned files. Offer path repair.
* **Place cleanup:** Detect duplicate/variant place entries, offer merge. Enrich from GeoNames (coordinates, hierarchy, alternate names).
* **Duplicate person detection:** Beyond the AI deduplicator agent — a deterministic tool that flags persons with similar names, dates, and locations for manual review.
* **Type cleanup:** Remove unused custom event/attribute/relationship types that accumulate from imports.

**Rich filter/query system:** Combinable filter rules beyond text search:
* Relationship filters: persons within N degrees, patrilineal/matrilineal lines, X-chromosomal inheritance path.
* Event filters: persons with/without specific event types, date ranges, places.
* Citation filters: persons with fewer than N citations (under-sourced).
* DNA filters: persons in a shared segment group, match threshold.
* Filters are saveable, nameable, and composable (AND/OR). Used by both UI views and batch operations.

**Undo/redo:** UI supports undo/redo via the audit log. Each user action is an undoable operation. `Ctrl-Z` reverts the last mutation.

---

#### 11. Agent System (Future — Pluggable)

##### 11.1 Agent Types

Agents are external processes that consume the HTTP API and submit proposals. They can be implemented in any language and take several forms:

**Coded agents** — programs with custom logic (API calls, data processing). Typically Python for LLM work, but can be Rust, Go, or any language with an HTTP client:

| Agent | Subscribes To | Action |
|---|---|---|
| Discoverer | User request or `entity.created` | Query FamilySearch/Discovery APIs, propose record matches above confidence threshold. |
| Document Processor | `media.uploaded` | Call `/media/{id}/extract`, review and refine results, submit structured proposals. |

**Prompt-only agents** — Defined entirely in YAML, no coding required. A runtime process (any language — the reference implementation is Python, but a Rust runtime is a future option) loads the prompt, injects context from the API, calls the LLM, and submits proposals:

```yaml
# agents/definitions/validator.yaml
name: validator
description: Check temporal, geographic, and genealogical constraints
subscribes_to: [entity.created, entity.updated]
model: gemini-2.5-flash   # default LLM; overridable per agent
context:
  - entity: "{{event.entity}}"           # the entity that triggered
  - ancestors: "{{entity.ancestors(3)}}"  # 3 generations of context
  - descendants: "{{entity.descendants(2)}}" # children, grandchildren
  - events: "{{entity.events}}"           # related events
  - family: "{{entity.families}}"         # all family groups
prompt: |
  You are a genealogy data validator. Given the following person, their
  family, and related events, check for ALL of the following contradiction
  types. Be thorough — these are real-world patterns that corrupt trees:

  TEMPORAL PARADOXES:
  - Birth date after death date
  - "Life after death" — person appears in records (census, marriage, etc.)
    dated AFTER their recorded death. Flag with high confidence.
  - Marriage before age 14 or after death
  - Child born before parent was 12 or after parent's death
  - Child born before parents' marriage by more than 9 months AND marriage
    date is suspiciously late — flag as "possible wrong family assignment"
    (e.g., child born 1866, parents' marriage 1887 = likely wrong couple)

  PARENTAL TIMELINE SANITY:
  - Mother giving birth after age 50 or before age 12
  - Father siring children after age 75 or before age 12
  - Children's birth dates imply impossible spacing (e.g., two births
    fewer than 9 months apart to the same mother)
  - Large gaps in children's birth years may indicate a missing child or
    a second marriage

  GEOGRAPHIC IMPOSSIBILITIES:
  - Person recorded in two distant locations within an impossibly short
    timeframe (consider era-appropriate travel — 1850s rural Wales is not
    the same as 1920s London)
  - Birthplace specificity mismatch: if a person self-reports a specific
    village (e.g., "Llanpumsaint") in one record but is assigned to a
    different parish in another, flag for review

  Person: {{context.entity}}
  Ancestors: {{context.ancestors}}
  Descendants: {{context.descendants}}
  Events: {{context.events}}
  Family groups: {{context.family}}

  For each issue found, return a JSON array of objects:
  {
    "entity_id": "...",
    "field": "...",
    "issue": "description of the contradiction",
    "severity": "error" | "warning" | "info",
    "confidence": 0.0-1.0,
    "suggested_action": "dispute" | "flag_for_review" | "suggest_split"
  }

  Return an empty array if no issues found.
output_schema: ValidationResult[]
action: submit_proposals   # auto-submit to staging queue
```

```yaml
# agents/definitions/deduplicator.yaml
name: deduplicator
description: Find potential duplicate persons
trigger: scheduled
schedule: "0 3 * * *"    # daily at 03:00
model: gemini-2.5-flash
context:
  - candidates: "{{search.similar_persons(threshold=0.6)}}"
prompt: |
  Compare the following pairs of person records and assess whether
  they are likely the same individual. Consider: name similarity,
  date proximity, geographic overlap, shared family members.

  Candidates: {{context.candidates}}

  For each pair, return:
  {
    "person1_id": "...",
    "person2_id": "...",
    "confidence": 0.0-1.0,
    "reasoning": "...",
    "suggested_action": "merge" | "link" | "ignore"
  }
output_schema: DeduplicationResult[]
action: submit_proposals
```

```yaml
# agents/definitions/naming-convention-inferrer.yaml
name: naming-convention-inferrer
description: Detect patronymic shifts, naming patterns, and suggest research directions
subscribes_to: [entity.created, entity.updated]
trigger: also_on_demand   # user can invoke manually for a stuck branch
model: gemini-2.5-pro     # needs stronger reasoning for cultural inference
context:
  - person: "{{event.entity}}"
  - children: "{{entity.children}}"
  - parents: "{{entity.parents}}"
  - siblings: "{{entity.siblings}}"
  - grandparents: "{{entity.ancestors(2)}}"
  - place: "{{entity.birth_place}}"
prompt: |
  You are a genealogy naming pattern analyst with expertise in British Isles
  naming conventions, especially Welsh, Scottish, and Irish patronymics.

  Given a person and their family context, analyse naming patterns:

  PATRONYMIC DETECTION:
  - If children's surname differs from father's surname but matches
    grandfather's FIRST name, flag as "patronymic shift" (e.g., David
    Richards' children surnamed Jones = grandfather likely named John/Jones)
  - If in Wales pre-1850, assume patronymics are likely even if records
    show fixed surnames — the transition was gradual and inconsistent

  NAMING HONOUR PATTERNS:
  - First son often named after paternal grandfather
  - First daughter often named after maternal grandmother
  - Children named after deceased siblings (replacement naming)
  - Children's given names that match a SURNAME in the family suggest
    maternal maiden name or other family connection (e.g., child named
    "Richard" in a Jones family → mother's maiden name may be Richards)

  RESEARCH SUGGESTIONS:
  - Based on detected patterns, suggest specific searches:
    "Child named Richard in Jones family — prioritise 'Richards' as
    maternal maiden name in parish searches"
  - Flag when a naming pattern breaks (may indicate adoption, step-parent,
    or second marriage)

  Consider the geographic and temporal context: {{context.place}} in the
  relevant era. Welsh, Scottish, Irish, and English naming customs differ.

  Person: {{context.person}}
  Children: {{context.children}}
  Parents: {{context.parents}}
  Siblings: {{context.siblings}}
  Grandparents: {{context.grandparents}}

  Return:
  {
    "patterns_detected": [
      {
        "pattern_type": "patronymic_shift" | "honour_naming" | "maternal_clue" | "replacement_naming" | "pattern_break",
        "description": "...",
        "confidence": 0.0-1.0,
        "research_suggestion": "specific actionable search to try",
        "affected_persons": ["uuid", ...]
      }
    ]
  }
output_schema: NamingPatternResult
action: submit_proposals
```

**Adding a new prompt-only agent** = writing a YAML file in `agents/definitions/` and restarting the agent runtime. No coding required. The runtime handles event subscription, LLM calls, output parsing, schema validation, and proposal submission.

##### 11.2 Agent Runtime Architecture

```
agents/
├── definitions/                 # YAML agent definitions (prompt-only agents)
│   ├── validator.yaml
│   ├── deduplicator.yaml
│   ├── naming-convention-inferrer.yaml
│   ├── relationship-inferrer.yaml
│   └── ...
├── python/                      # Python-based agents and YAML runtime
│   ├── pyproject.toml           # uv workspace root
│   ├── packages/
│   │   ├── runtime/             # Shared agent runtime (loads YAMLs, runs prompts)
│   │   │   ├── pyproject.toml
│   │   │   └── src/
│   │   │       ├── loader.py    # Parse YAML definitions
│   │   │       ├── context.py   # Template engine — resolves {{entity.ancestors(3)}} etc.
│   │   │       ├── llm.py       # LLM provider abstraction
│   │   │       └── submitter.py # Validates output, submits to /staging
│   │   ├── client/              # Auto-generated from OpenAPI spec
│   │   ├── discoverer/          # Coded agent (custom FamilySearch logic)
│   │   └── doc-processor/       # Coded agent (custom vision/OCR logic)
├── rust/                        # Rust-based agents (compiled into crates/agents/)
│   └── ...                      # Future: high-performance validators, batch processors
└── scripts/                     # Standalone scripts (shell, Python, etc.)
    └── ...                      # Lightweight, single-file agents
```

##### 11.4 Agent Contract and Packaging

Agents are **language-agnostic external processes**. The core does not care what language an agent is written in — only that it speaks the agent protocol. This section defines the universal contract and packaging strategy for each agent type.

**Universal agent contract:**

All agents, regardless of implementation language, interact with the core via:
1. **Authentication:** `Authorization: Bearer <api-key>` header on all HTTP requests.
2. **Read:** `GET` requests to the REST API (`/persons`, `/events`, `/search`, `/graph`).
3. **Subscribe:** SSE stream from `/events/stream` or polling `/events/poll`.
4. **Write:** `POST` proposals to `/staging` — never direct mutations.
5. **Health:** `POST /agents/{id}/heartbeat` at a configurable interval (default 30s).

The contract is defined by the OpenAPI spec. Any process that can make HTTP requests and parse JSON is a valid agent.

**Agent types and packaging:**

| Agent type | Implementation | Packaging for distribution | Example |
|---|---|---|---|
| **Prompt-only (YAML)** | YAML definition file, no code | Bundled as data files with the app. The runtime that executes them is a separate process. | `validator.yaml`, `naming-convention-inferrer.yaml` |
| **Python script** | Python source using the generated OpenAPI client | Compiled to standalone executable via `pyinstaller` or `python-build-standalone`. Bundled as a Tauri sidecar with target triple suffix (e.g., `agent-discoverer-aarch64-apple-darwin`). No Python installation required on the user's machine. | `discoverer`, `doc-processor` |
| **Rust binary** | Compiled Rust crate in `crates/agents/` or standalone | Native binary, bundled directly with the Tauri app or as a sidecar. Smallest footprint, fastest startup. | Future: high-performance batch validator |
| **External script** | Shell script, Node.js, Go binary, etc. | User-managed. Registered via `rustygene agent register --name <name> --command <path>`. Not bundled with the app. | User-written automation, CI/CD integrations |
| **YAML runtime** | The process that loads and executes YAML agent definitions | Reference implementation is Python (compiled to standalone binary). Future: Rust reimplementation for zero-dependency execution. | `agents/python/packages/runtime/` |

**Sidecar packaging (Tauri):** Agents distributed with the desktop app are registered as Tauri sidecars. Tauri sidecars must be named with a target triple suffix for cross-platform builds. On app launch, the core optionally starts registered sidecar agents as child processes. Agent lifecycle (start/stop/restart) is managed via the UI or CLI (`rustygene agent start <name>`, `rustygene agent stop <name>`).

**User-installed agents:** Third-party or user-written agents are registered at runtime via `rustygene agent register --name <name> --command "/path/to/agent"`. The core spawns the command, passes config (API URL, API key, subscriptions) via environment variables or a JSON config file, and monitors the process. User agents are sandboxed by the same staging queue constraint — they can only propose, never mutate directly.

##### 11.3 LLM Provider Configuration

**Default LLM:** Gemini (2.5 Flash for cost efficiency, 2.5 Pro for complex reasoning). Gemini is the default because of its strong structured output support, competitive pricing, and generous free tier for development.

**Multi-provider support:** The LLM abstraction layer supports:
* **Gemini** (default) — via `google-genai` SDK
* **Claude** — via `anthropic` SDK
* **OpenAI-compatible** — via `openai` SDK (covers GPT, Groq, Together, etc.)
* **Local models** — via Ollama (LLaMA, Mistral, etc.) for offline/privacy-sensitive use

Provider is configurable per-agent in YAML (`model: gemini-2.5-flash`, `model: claude-sonnet-4-6`, `model: ollama/llama3.2`). Global default set in `agents/config.yaml`.

---

#### 12. Language Standards & Tooling

##### Rust
* **Edition:** 2024 (Rust 1.85+). Use `edition = "2024"` in all `Cargo.toml` files.
* **MSRV:** 1.85.0 (first edition 2024 release).
* **Style:** `rustfmt` with default settings. `clippy` with `#![warn(clippy::all, clippy::pedantic)]`.
* **Async:** Tokio runtime. `async fn` in traits (stabilised in Rust 2024, no `#[async_trait]` needed).
* **Error handling:** `thiserror` for library errors, `anyhow` for application/CLI errors. No silent swallowing — fail fast and loud.
* **Testing:** `cargo test` with `#[test]` and `#[tokio::test]`. Integration tests in `tests/` directory per crate. Property-based testing with `proptest` for the domain model (date parsing, name matching).
* **CI:** `cargo fmt --check`, `cargo clippy`, `cargo test`, `cargo doc` on every PR.

##### Python (Agents)
* **Version:** 3.12+ (structural pattern matching, modern generics, `type` statement).
* **Package manager:** `uv`. All commands via `uv run`.
* **Linter/formatter:** `ruff` (format + lint). Configuration in `pyproject.toml`.
* **Type checking:** `pyright` or `ty` in strict mode. No `Any` unless forced by an external library.
* **Models:** Pydantic v2 `BaseModel` for all data structures.
* **Testing:** `pytest` with `pytest-asyncio`. Fixtures for API client mocking.

##### Frontend (Svelte)
* **Svelte 5** with runes (`$state`, `$derived`, `$effect`). No legacy Svelte 4 patterns.
* **TypeScript** strict mode. No `any`.
* **Package manager:** `pnpm` (preferred for monorepo workspaces) or `npm`.
* **Formatter:** `prettier` + `eslint`.

---

#### 13. Testing Strategy

Testing is not an afterthought — it is the primary mechanism for validating domain model correctness and preventing regressions in a system where data integrity is paramount.

##### 13.1 Test Pyramid

| Layer | Tool | What | Coverage Target |
|---|---|---|---|
| **Unit** | `cargo test`, `pytest` | Individual functions: date parsing, name matching, assertion conflict logic, calendar conversion, phonetic encoding | Domain model: 90%+ line coverage |
| **Property-based** | `proptest` (Rust), `hypothesis` (Python) | Invariant testing: "for any valid DateValue, serialize → deserialize is identity", "for any two Names, similarity score is symmetric", "for any assertion set, exactly one is preferred" | All core domain types |
| **Integration** | `cargo test` (in `tests/`) | Storage layer: CRUD operations, FTS5 indexing, optimistic locking conflicts, graph traversal CTEs, constraint enforcement | Every SQL table and index |
| **GEDCOM round-trip** | Dedicated test suite | Import reference GEDCOM files → export → diff. Any delta is a bug. Test corpus: Gramps sample files, TNG samples, known edge-case files from GEDCOM-L mailing list | 100% tag coverage for GEDCOM 5.5.1 |
| **API contract** | `reqwest` test client against Axum | Every REST endpoint: happy path, validation errors (422), optimistic lock conflicts (409), auth failures (401/403), pagination, filtering | Every endpoint |
| **Agent** | `pytest` with mocked API | Agent runtime: YAML loading, context template resolution, LLM response parsing, proposal submission, idempotency dedup | All YAML agents |
| **UI** | Playwright or Vitest | Critical workflows: person CRUD, review queue approve/reject, search, GEDCOM import wizard | Smoke coverage of primary flows |

##### 13.2 Test Data

* **`testdata/` directory** committed to the repo. Contains:
    * Reference GEDCOM files (small, medium, edge-case).
    * Sample media files (JPEG, PNG, PDF) for media pipeline tests.
    * CSV census extracts for bulk import testing.
    * Golden-file JSON snapshots for serialization tests.
* **Fixture generation:** Use `proptest` to generate randomised but valid entity graphs for stress testing storage and graph traversal.
* **LLM test fixtures:** Recorded LLM responses (not live calls) for agent tests. Agents are tested against deterministic fixtures, not live LLM APIs. Live integration tests run separately in CI with a dedicated test budget.

##### 13.3 CI Pipeline

```
cargo fmt --check
cargo clippy -- -D warnings
cargo test                         # unit + integration + property + GEDCOM round-trip
uv run ruff check agents/          # Python lint
uv run ruff format --check agents/
uv run pyright agents/             # type check
uv run pytest agents/              # agent unit tests
pnpm -C app lint                   # frontend lint
pnpm -C app test                   # frontend unit tests
```

All checks must pass before merge. No skipping, no `#[ignore]` without a tracking issue.

##### 13.4 Key Test Scenarios (Domain-Specific)

These are non-obvious edge cases that must have explicit tests before implementation is considered complete:

* **Date parsing:** "BET 1850 AND 1855", "ABT 1868", "Q1 1881", "3 FEB 1723/24" (dual date), "@#DJULIAN@ 15 OCT 1582", "@#DFRENCH R@ 1 VENDEMIAIRE AN II", empty/null dates, impossible dates (31 Feb).
* **Name matching:** Smith ↔ Smyth (phonetic), Elisabeth ↔ Elizabeth (alternate), Wm ↔ William (known abbreviation), "de la Cruz" ↔ "Cruz" (prefix handling), "García y López" (compound surname), "Björk Guðmundsdóttir" (Icelandic patronymic).
* **Assertion conflicts:** Two `Confirmed` assertions for the same field with different values. Preferred marking. Confidence ranking. Dispute workflow.
* **Pedigree collapse:** Person appears as both great-grandparent via two paths. Graph traversal must not infinite-loop or double-count.
* **Optimistic locking:** Two concurrent updates to the same entity — second must get 409.
* **GEDCOM edge cases:** Empty `NOTE` tags, multi-line `CONT`/`CONC`, UTF-8 BOM, ANSEL encoding, nested `SOUR` citations, custom `_CUSTOM` tags preserved in round-trip.
* **Privacy:** Export with living-person redaction. Verify names replaced, events stripped, structure preserved. Re-import the redacted export — must not corrupt.
* **Calendar conversion:** Julian date in a British record pre-1752 → display as Gregorian equivalent. Dual date "1723/24" → both years preserved.

---

#### 14. Work Structuring Guidance

##### Development approach
* **Test-driven for the domain model.** Write tests for `DateValue` parsing, name matching, assertion conflict resolution, and GEDCOM import before writing the implementation. The domain model is the foundation — get it right early.
* **CLI-first.** Phase 1 delivers a CLI that validates every domain model decision. If the CLI can import a GEDCOM file, query persons, and export cleanly, the model is sound. The UI and API are presentation layers over a proven core.
* **One crate at a time.** Start with `crates/core` (pure domain types, zero dependencies). Then `crates/storage` (SQLite). Then `crates/gedcom` (import/export). Then `crates/api` (REST). Each crate has a clear boundary and can be tested independently.
* **Beads for issue tracking.** Use `bd` to track issues, tasks, and progress. Issues are prefixed `rustygene-<hash>`.

##### PR discipline
* Small, focused PRs. One crate or one feature per PR.
* Every PR must pass `cargo fmt --check`, `cargo clippy`, `cargo test`.
* OpenAPI spec regenerated and committed whenever `crates/api` changes.

##### When to use AI assistance
* Domain model design — use LLMs to review struct designs against GEDCOM X / Gramps models.
* GEDCOM edge case handling — GEDCOM files are notoriously inconsistent; use LLMs to generate test fixtures for weird edge cases.
* Agent prompt engineering — iterate on agent prompts with real genealogy data.
* Do NOT use AI for core storage logic or security-sensitive code (auth, data integrity).

##### Retrospectives & continuous improvement
After each phase completion (and at regular intervals during development), AI agents assisting with development must produce a structured retrospective:

* **What went well** — approaches, patterns, tools, or decisions that proved effective. Capture these so they are repeated.
* **What went badly** — mistakes, false starts, rework, misunderstandings. Capture root causes, not just symptoms.
* **What to change** — concrete, actionable adjustments for the next phase.
* **Surprises** — anything unexpected that was learned about the domain, the tooling, or the codebase.

Retrospectives are stored in `docs/retro/` as dated markdown files (`docs/retro/2026-03-29-phase1-core.md`). They are living documents — append to them as new learnings emerge. The goal is continuous improvement of both the codebase and the development process itself. Do not treat this as ceremony — a five-line retro that captures something real is worth more than a page of boilerplate.

---

#### 15. Dependency Summary

| Component | Crate / Package | Status | Risk |
|---|---|---|---|
| Web framework | `axum` 0.8 | Mature, Tokio-backed | Low |
| OpenAPI | `utoipa` 5.x | Mature, high adoption | Low |
| GEDCOM parse | `ged_io` 0.12 | Active, pre-1.0 | Medium — pin version, wrap in trait |
| GEDCOM X types | `gedcomx` 0.1.7 | Dormant, stable | Low |
| Database (local) | `rusqlite` or `sqlx` + SQLite | Mature | Low |
| SQL migrations | `refinery` | Mature, supports rusqlite + sqlx | Low |
| Database (server) | `sqlx` + PG + Apache AGE | AGE: small team, irregular releases | Medium |
| Full-text search | SQLite FTS5 / PG tsvector | Built-in | Low |
| Desktop shell | Tauri 2.x | Stable, production-ready | Low |
| UI framework | Svelte 5 | Stable | Low |
| Graph viz | Cytoscape.js 3.33 | Mature | Low |
| Charts | D3.js | Mature | Low |
| Serialization | `serde` + `serde_json` | Mature | Low |
| HTTP client | `reqwest` | Mature | Low |
| XML parsing | `quick-xml` | Mature | Low |

---

#### 16. Phasing

##### Phase discipline

The spec describes a platform. Building it all in one wave is a delivery risk. The phases below are ordered so that each one validates the architecture before the next adds complexity. **Phase 1A is the only scope that matters until it ships.**

| Phase | Scope | Explicitly deferred to later phase |
|---|---|---|
| **1A** | Rust core domain model (all entity types, assertion wrapper), SQLite storage (schema, CRUD, audit log), GEDCOM 5.5.1 import (semantic fidelity — see §7.1), GEDCOM 5.5.1 export (best-effort, not perfect round-trip yet), CLI (`import`, `export`, `query`, `show`, `search`), JSON export, core validation rules, research log (CLI) | Sandboxes (data model columns OK, no UI/logic), connectors, agents, REST API, event bus, TUI, desktop app, DNA, Gramps XML import |
| **1B** | GEDCOM round-trip hardening (corpus testing, tag coverage), Gramps XML import, GEDCOM merge import — deterministic matching engine + CLI `diff`/`merge` modes (§7.2), FTS5 search with phonetic/fuzzy matching, generated columns for hot query fields, sandbox assertion overlay logic (no UI), staging queue (internal, no API) | Sandbox UI, REST API, agents, connectors, desktop app |
| **2** | Tauri desktop app (Svelte 5 + Cytoscape.js + D3.js), person/family/event CRUD, pedigree + fan chart + graph views, document attachment + viewer, full-text search UI, backup/restore, source-driven form entry, session restore | Sandbox UI, REST API, agents, connectors |
| **3** | Research sandbox UI (create/switch/compare/promote/discard), Axum REST API + OpenAPI spec, event bus (internal channels + SSE/polling), staging queue + review dashboard, agent registry, sandbox comparison + validator scoring, negative evidence prompt | Connectors, agents |
| **4** | FamilySearch connector, Discovery API connector, document processor (vision/OCR), validator agent, naming convention inferrer agent | Server edition, DNA integration |
| **5** | Discoverer agent, deduplicator agent, PostgreSQL + Apache AGE backend, multi-user collaboration, S3 media storage, DNA data integration |  |

**Phase 1A — the definition of "done":**
1. `rustygene import --format gedcom tests/data/sample.ged` loads a GEDCOM 5.5.1 file into SQLite, creating assertions for every fact.
2. `rustygene export --format gedcom` writes a GEDCOM file. Core records (INDI, FAM, SOUR, REPO, NOTE, OBJE) are semantically faithful. Unknown/custom tags preserved via `_raw_gedcom`. Textual byte-for-byte identity is not required yet.
3. `rustygene export --format json` dumps the full assertion-based model.
4. `rustygene query person --name "Jones"` returns matching persons with their preferred assertions.
5. `rustygene show person <id>` displays a person with all assertions, sources, and linked events.
6. Core validation rules reject impossible data (birth after death, self-parentage, impossible dates) at the storage layer.
7. Audit log records every mutation.
8. Research log entries can be created via CLI.
9. Property-based tests cover domain model invariants. Integration tests cover every SQL table. GEDCOM import/export has a test suite against reference files.

##### 16.1 Phase 1A Implementation Sequence

Ordered by dependency. Each sub-step is independently testable and sized for a single Beads issue (1-3 days). Dependency annotations show what must complete before a sub-step can start. Sub-steps within the same step that share no dependency can be parallelised.

---

**Step 1 — Workspace scaffold and project foundations**

| Sub-step | Description | Depends on | Deliverable |
|---|---|---|---|
| 1.1 | Create Cargo workspace root `Cargo.toml`. Create `crates/core/`, `crates/storage/`, `crates/gedcom/`, `crates/cli/` with stub `Cargo.toml` and `lib.rs`/`main.rs`. Wire dependency direction: `cli` → `gedcom` → `storage` → `core`. Verify `cargo build` succeeds. | — | Compiling workspace with 4 crates |
| 1.2 | CI pipeline: GitHub Actions workflow running `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` on every push/PR. | 1.1 | Green CI on push |
| 1.3 | Acquire reference GEDCOM 5.5.1 test files. At minimum: one small clean file (~50 persons), one medium file (~500 persons from a real export), one with known edge cases (custom tags, LDS ordinances, multi-byte UTF-8, NOTE continuations). Place in `testdata/gedcom/`. | — | `testdata/` populated |
| 1.4 | Write `CLAUDE.md` with: build commands, crate boundaries, dependency direction, naming conventions, test commands, CI expectations, coding style (edition 2024, clippy pedantic, no `Any`). | 1.1 | `CLAUDE.md` in repo root |

---

**Step 2 — Domain model (`crates/core`)**

| Sub-step | Description | Depends on | Deliverable |
|---|---|---|---|
| 2.1 | **Foundation types.** `EntityId` (Uuid newtype), `DateValue` enum (Exact, Range, Before, After, About, Quarter, Textual, Tolerance) with `serde`, `Display`, `PartialOrd` (where meaningful). `Gender` enum. `ActorRef` (user/agent/import discriminated string). | 1.1 | `crates/core/src/types.rs` compiles, unit tests for DateValue ordering and serde round-trip |
| 2.2 | **Person and Name types.** `Person`, `PersonName`, `Surname`, `NameType`, `SurnameOrigin` with serde. Implement `PersonName::sort_key()` respecting `sort_as` override. | 2.1 | Unit tests for name sort key, serde round-trip of complex multi-surname names |
| 2.3 | **Place types (Phase 1A subset).** `Place`, `PlaceName` (no date_range yet), `PlaceRef` (single parent, `hierarchy_type` defaults to Admin, no date_range), `PlaceType`, `HierarchyType`. Full struct definitions exist but optional temporal fields are always `None` in Phase 1A. | 2.1 | Unit tests for Place serde |
| 2.4 | **Event types.** `Event`, `EventType` (exhaustive enum: Birth, Death, Marriage, Census, Baptism, Burial, Migration, Occupation, Residence, Immigration, Emigration, Naturalization, Probate, Will, Graduation, Retirement, Custom(String)...), `EventParticipant`, `EventRole`, `CensusRole`. | 2.1, 2.3 | Unit tests for event participant role typing |
| 2.5 | **Family and Relationship types.** `Family`, `ChildLink`, `LineageType`, `Relationship`, `RelationshipType`, `PartnerLink`. Implement Principle 2 linking rules as doc comments + type constraints (Family references Relationship by ID, Relationship references Event by ID). | 2.1, 2.4 | Unit tests for Family↔Relationship↔Event reference integrity |
| 2.6 | **Evidence chain types.** `Repository`, `Source`, `Citation`, `CitationRef`, `Media`, `MediaRef` (with crop rectangle), `Note`, `NoteRef`. | 2.1 | Serde round-trip tests for the full Repository→Source→Citation chain |
| 2.7 | **Assertion and Sandbox types.** `Assertion<T>`, `AssertionStatus`, `EvidenceType`, `Sandbox`, `SandboxStatus`. Implement idempotency key computation: `hash(entity_id + field + value + sorted(source_citations))` — explicitly excluding metadata. | 2.1, 2.6 | Unit tests for idempotency key: same fact from different agents → same key; different facts → different keys |
| 2.8 | **LDS ordinance types.** `LdsOrdinance`, `LdsOrdinanceType`, `LdsStatus` (20+ status values). | 2.1 | Serde round-trip, exhaustive LdsStatus enum |
| 2.9 | **Research log types.** `ResearchLogEntry`, `SearchResult` enum. | 2.1, 2.6 | Serde round-trip |
| 2.10 | **Core validation functions.** `validate_birth_before_death()`, `validate_parent_age_gap()`, `validate_date_possible()`, `validate_no_self_parentage()`, `validate_event_within_lifespan()`. Each returns a typed `ValidationError`. Functions are pure — they take domain types in, return `Result`. | 2.1-2.5 | Unit tests for each validation rule with both valid and invalid inputs |
| 2.11 | **Property-based tests.** `proptest` strategies for: arbitrary `DateValue` (fuzz ordering invariants), arbitrary `Assertion<T>` (status transition validity), arbitrary `PersonName` (serde round-trip), arbitrary entity reference graphs (no dangling refs after construction). | 2.1-2.10 | `proptest` suite in `crates/core/tests/` — passing, no regressions |

---

**Step 3 — Storage layer (`crates/storage`)**

| Sub-step | Description | Depends on | Deliverable |
|---|---|---|---|
| 3.1 | **Storage trait definition.** Define `trait Storage` with async method signatures for: entity CRUD (all 9 entity types), assertion CRUD, research log CRUD, audit log append, graph edge queries. Use `core` types as parameters/returns. No implementation yet. | 2.1-2.9 | `crates/storage/src/traits.rs` compiles |
| 3.2 | **SQLite schema migration setup.** Integrate `refinery`. Write initial migration (`V001__initial_schema.sql`) creating all tables from §4.2: persons, families, events, places, sources, citations, repositories, media, notes, assertions (with `sandbox_id`), relationships (with `directed` flag), audit_log, event_log, research_log, sandboxes, agents. Include all indexes. | 3.1 | `migrations/V001__initial_schema.sql` applied successfully on fresh database. Test: create db, verify all tables exist. |
| 3.3 | **Entity CRUD implementation.** Implement `Storage` trait methods for insert/get/update/delete/list on all 9 entity tables. JSON serialisation of domain types into `data` column. Optimistic locking: update requires `WHERE version = ?`, returns error on mismatch. | 3.2 | Integration tests: insert, get, update (version increments), optimistic lock conflict returns error, delete, list with pagination |
| 3.4 | **Assertion CRUD implementation.** Create assertion (with idempotency key check — duplicate key returns existing assertion, not error). Query by entity_id + field. Query by entity_id (all fields). Update status. Set/clear preferred flag. Filter by status. | 3.2, 2.7 | Integration tests: create, duplicate detection, query, status update, preferred flag, filter |
| 3.5 | **Snapshot recomputation (write-through).** On every assertion mutation (create, status change, preferred change), recompute the affected entity's JSON snapshot from current `Confirmed` + `preferred` assertions. Write back to entity table in the same transaction. Implement `rebuild_all_snapshots()` for bulk regeneration. | 3.3, 3.4 | Integration test: create entity, add assertions, verify snapshot reflects preferred assertion. Change preferred, verify snapshot updates. `rebuild_all_snapshots()` produces identical result. |
| 3.6 | **Audit log implementation.** Append-only. Every entity and assertion mutation records: timestamp, actor, entity_id, entity_type, action, field-level diff (not full snapshots — see §4.2). | 3.3, 3.4 | Integration test: perform mutations, query audit log, verify all mutations recorded with correct diffs |
| 3.7 | **Research log CRUD.** Insert, list (with filters: by person, by date range, by result type), get by ID. | 3.2 | Integration tests for research log CRUD and filtering |
| 3.8 | **Relationship graph edges.** Insert/query the `relationships` table. Directed edges (parent_of) queried from→to. Undirected edges (partner_in) stored once (lower UUID first), queried on either side. Graph traversal: ancestors(n), descendants(n) via recursive CTE. | 3.2 | Integration tests: insert directed + undirected edges, query both directions for undirected, ancestors(3), descendants(2) |
| 3.9 | **Storage integration test suite.** End-to-end test: create a small family (2 parents, 3 children, 1 marriage event, 2 sources, 3 citations), verify all entities, assertions, relationships, audit log, and snapshots are correct. Test optimistic lock conflict. Test assertion idempotency. | 3.3-3.8 | Comprehensive integration test in `crates/storage/tests/` |

---

**Step 4 — GEDCOM import (`crates/gedcom`)**

| Sub-step | Description | Depends on | Deliverable |
|---|---|---|---|
| 4.1 | **GEDCOM line tokeniser.** Parse GEDCOM text into `Vec<GedcomLine>` where each line is `(level: u8, xref: Option<String>, tag: String, value: Option<String>)`. Handle CONC/CONT continuation lines. Handle BOM. Handle line endings (CR, LF, CRLF). | 1.1 | Unit tests: parse well-formed lines, continuation lines, BOM handling, edge cases (empty values, multi-byte UTF-8) |
| 4.2 | **Tree builder.** Convert flat `Vec<GedcomLine>` into a hierarchical tree of `GedcomNode` (tag, value, children). Level 0 nodes are roots (HEAD, INDI, FAM, SOUR, REPO, NOTE, OBJE, TRLR). | 4.1 | Unit tests: well-formed GEDCOM → correct tree structure, verify parent-child relationships |
| 4.3 | **Entity mapper — Person.** Map INDI nodes to `Person` + `PersonName` + names/gender/events. Extract birth date, death date, name parts (GIVN, SURN, NPFX, NSFX, _MARNM). Handle multiple NAME records. | 4.2, 2.2 | Unit test: parse a reference INDI record, verify Person struct fields |
| 4.4 | **Entity mapper — Family + Relationship + Event.** Map FAM nodes to `Family` + `Relationship` (couple) + child links. Map MARR/DIV events to `Event` entities. Apply Principle 2 invariants: marriage Event → couple Relationship → Family. | 4.2, 2.4, 2.5 | Unit test: parse a FAM record with HUSB/WIFE/CHIL/MARR, verify all three entity types created with correct references |
| 4.5 | **Entity mapper — Source chain.** Map SOUR → `Source`, REPO → `Repository`, inline source citations within INDI/FAM records → `Citation` + `CitationRef`. Map source-within-source (SOUR.SOUR) nesting. | 4.2, 2.6 | Unit test: parse SOUR/REPO records, verify Repository→Source→Citation chain |
| 4.6 | **Entity mapper — Media, Note, LDS.** Map OBJE → `Media`, NOTE → `Note` (handle inline and referenced notes). Map BAPL/ENDL/SLGC/SLGS → `LdsOrdinance`. | 4.2, 2.6, 2.8 | Unit tests for each entity type |
| 4.7 | **Assertion generation.** Wrap every extracted fact in `Assertion<T>` with `proposed_by: import:<job_id>`, `status: Confirmed`, `evidence_type: Direct`, `confidence: 1.0`. Citation propagation: Event assertions carry the Event's citations; all participant person assertions inherit them (§3.1 citation propagation rule). | 4.3-4.6, 2.7 | Unit test: verify assertion metadata, citation propagation to participants |
| 4.8 | **Unknown tag preservation.** Any GEDCOM tag not recognised by the mapper is stored verbatim in `_raw_gedcom: Map<String, String>` on the nearest parent entity. Includes the full subtree (tag + value + children serialised). | 4.2 | Unit test: parse GEDCOM with custom tags (_LOC, _MILT, vendor extensions), verify they appear in `_raw_gedcom` |
| 4.9 | **Import pipeline.** Wire tokeniser → tree builder → entity mappers → assertion generator → `SqliteStorage` in a single database transaction. Report: entities created (by type), assertions created, unknown tags preserved. | 4.1-4.8, 3.9 | Integration test: import reference GEDCOM file, verify entity counts match expected, spot-check 5+ assertion values |
| 4.10 | **Import edge case tests.** Test with: empty GEDCOM (HEAD + TRLR only), GEDCOM with only sources (no persons), GEDCOM with deep nesting, GEDCOM with LDS ordinances, GEDCOM with non-ASCII characters, GEDCOM with NOTE continuations spanning 10+ lines. | 4.9, 1.3 | All edge case tests passing |

---

**Step 5 — GEDCOM export (`crates/gedcom`)**

| Sub-step | Description | Depends on | Deliverable |
|---|---|---|---|
| 5.1 | **Entity-to-GEDCOM renderer.** Convert each entity type back to GEDCOM node trees: Person → INDI, Family → FAM, Source → SOUR, Repository → REPO, Note → NOTE, Media → OBJE. Collapse assertions to single values (preferred, or highest-confidence Confirmed). | 4.3-4.6 | Unit tests: render a Person with known assertions, verify GEDCOM tag structure |
| 5.2 | **Unknown tag re-emission.** Append `_raw_gedcom` entries as GEDCOM subtrees at the correct position within each entity's output. | 5.1 | Unit test: entity with _raw_gedcom entries → GEDCOM output contains those tags |
| 5.3 | **GEDCOM file writer.** Emit HEAD record (with SOUR, GEDC version, CHAR UTF-8), all entity records, TRLR. Handle CONC/CONT for long values. Ensure correct level numbering. | 5.1, 5.2 | Unit test: write a complete GEDCOM file, verify syntactic correctness |
| 5.4 | **Privacy redaction.** Living persons (§3.7) redacted on export: name → "Living", events stripped, node preserved for structural integrity. Private entities excluded entirely. Export accepts a redaction policy parameter. | 5.1 | Unit test: export with living person → redacted output. Export with private entity → entity absent. |
| 5.5 | **Semantic round-trip test.** Import reference GEDCOM → export → re-import into a fresh database → compare assertion graphs. Every assertion in the original must exist in the re-imported database with identical entity_id mapping, field, value, and source citations. This is the Phase 1A fidelity gate. | 4.9, 5.3 | Round-trip test passing for all reference GEDCOM files in `testdata/` |

---

**Step 6 — JSON export and import**

| Sub-step | Description | Depends on | Deliverable |
|---|---|---|---|
| 6.1 | **JSON export.** Full-fidelity dump: all entities (with all assertions, not just preferred), audit log, research log. Output as a directory of JSON files (one per entity type) or a single JSON file (configurable). Includes a manifest with export timestamp, entity counts, schema version. | 3.9 | Integration test: export populated database, verify file structure and content |
| 6.2 | **JSON import.** Rebuild SQLite database from a JSON export. Create entities and assertions from the export data. Validate referential integrity during import. | 6.1 | Integration test: export → import into fresh db → verify identical entity and assertion counts |
| 6.3 | **JSON round-trip test.** Export → import → export again → diff the two exports. They must be identical. | 6.1, 6.2 | Round-trip test passing |

---

**Step 7 — CLI (`crates/cli`)**

| Sub-step | Description | Depends on | Deliverable |
|---|---|---|---|
| 7.1 | **CLI scaffold.** `clap` derive-based setup. Subcommand structure: `import`, `export`, `query`, `show`, `research-log`, `rebuild-snapshots`. Global flags: `--db <path>` (database location, default `~/.rustygene/rustygene.db`), `--format json\|text` (output format). | 1.1 | `rustygene --help` prints usage |
| 7.2 | **Import command.** `rustygene import --format gedcom <file>` — calls GEDCOM import pipeline. `rustygene import --format json <dir>` — calls JSON import. Reports entity/assertion counts on success. | 4.9, 6.2 | CLI test: import reference GEDCOM, verify success output |
| 7.3 | **Export command.** `rustygene export --format gedcom [--output <file>]`, `rustygene export --format json [--output <dir>]`. Default output to stdout (GEDCOM) or current directory (JSON). `--redact-living` flag for privacy. | 5.3, 6.1 | CLI test: export to file, verify file exists and is valid |
| 7.4 | **Query command.** `rustygene query person --name <pattern>` — exact match search against person name assertions. Returns list of matching persons with ID, preferred name, birth/death dates. `--format json` for machine-readable output. | 3.3, 3.4 | CLI test: import GEDCOM, query by name, verify results |
| 7.5 | **Show command.** `rustygene show person <id>` — display person with all assertions (grouped by field), linked events, families, sources. `rustygene show family <id>` — display family with partners, children, linked events. `rustygene show event <id>` — display event with participants. | 3.3, 3.4, 3.8 | CLI test: import GEDCOM, show a person, verify all sections present |
| 7.6 | **Research log commands.** `rustygene research-log add --objective "..." --result found\|not_found\|partially_found\|inconclusive [--person <id>] [--repository <id>]`. `rustygene research-log list [--person <id>] [--result <type>]`. | 3.7 | CLI test: add entry, list entries, filter by person |
| 7.7 | **Rebuild-snapshots command.** `rustygene rebuild-snapshots` — regenerate all entity snapshots from assertions. Safety net for snapshot consistency. Reports count of entities rebuilt. | 3.5 | CLI test: import, rebuild, verify no changes (idempotent) |

---

**Step 8 — Hardening and acceptance**

| Sub-step | Description | Depends on | Deliverable |
|---|---|---|---|
| 8.1 | **Real-world GEDCOM corpus testing.** Test import/export with at least 3 GEDCOM files from different vendors: Ancestry export, RootsMagic export, Gramps export. Identify and catalogue failures. | 4.9, 5.5 | Test report: which files pass/fail, which tags are unhandled |
| 8.2 | **Edge case fixes.** Fix import/export failures found in 8.1. Track remaining known gaps (to be addressed in Phase 1B) in a `docs/GEDCOM_GAPS.md` file. | 8.1 | Updated code, `GEDCOM_GAPS.md` documenting known limitations |
| 8.3 | **End-to-end acceptance test.** Single automated test that exercises the full pipeline: `import GEDCOM` → `query person` → `show person` → `export GEDCOM` → `re-import` → `compare assertion graphs` → `export JSON` → `re-import JSON` → `compare`. This is the Phase 1A gate test. | 7.2-7.5, 5.5, 6.3 | Acceptance test passing |
| 8.4 | **Documentation.** Verify `CLAUDE.md` is current. Write `docs/ARCHITECTURE.md` documenting actual crate structure as built. Create initial `docs/DECISIONS.md` for any deviations from this spec encountered during implementation. | 8.3 | All docs committed |

---

**Dependency summary (critical path):**

```
1.1 → 2.1 → 2.2-2.9 (partially parallel) → 2.10-2.11 → 3.1 → 3.2 → 3.3-3.8 (partially parallel)
→ 3.9 → 4.1 → 4.2 → 4.3-4.8 (partially parallel) → 4.9 → 4.10
→ 5.1-5.4 (partially parallel) → 5.5 → 7.1-7.7 (partially parallel) → 8.1 → 8.2 → 8.3 → 8.4
```

Steps 6.1-6.3 (JSON export/import) can run in parallel with Steps 5.x once Step 3.9 completes.
Steps 1.2-1.4 can run in parallel with Step 2.x.

**Total sub-steps: 48.** At 1-3 days each, Phase 1A is roughly 2-4 months of focused solo development, or 4-8 weeks with two developers working the parallel paths.

---

#### 17. Open Questions

1. ~~**Licensing?**~~ **Resolved: MIT/Apache dual license.** Maximises adoption and compatibility with volunteer genealogy communities.
2. ~~**Collaboration model?**~~ **Resolved: Single-user desktop for Phases 1-4.** Multi-user collaboration is a long-term goal (Phase 5+). The data model (assertions, audit log, optimistic locking) is designed to support it when the time comes.
3. **OpenCLAW integration?** Potential scope extension — legal document processing for probate/will records. Deferred.
4. ~~**Research branching?**~~ **Elevated to Phase 2-3. See §3.8 Research Sandboxes below.**
5. **GEDCOM 7.0 import priority?** Future standard but low current adoption.
6. **Offline-first guarantee?** Core app works fully offline (SQLite + local media). Network only for connectors/agents.
7. ~~**Living persons privacy?**~~ **Resolved in §3.7 Design Constraints.** `living: bool` flag on Person with configurable age threshold, export redaction, GDPR notes.
