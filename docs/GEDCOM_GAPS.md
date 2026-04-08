# GEDCOM 5.5.1 Import/Export Gaps

Known limitations in GEDCOM handling, discovered during Phase 1A/1B testing with
`testdata/gedcom/kennedy.ged`, `simpsons.ged`, `torture551.ged`, and the
Phase 1B corpus fixtures (`ancestry_sample.ged`, `rootsmagic_sample.ged`,
`gramps_sample.ged`, `legacy_sample.ged`, `paf_sample.ged`).

Last reviewed: 2026-04-08.

---

## Resolved Gaps (kept for history)

### ~~1. Person Events Not Exported to INDI Nodes~~ — FIXED

`person_to_indi_node_with_policy` now accepts `&[Event]` and `&[Place]` and
emits BIRT/DEAT/BURI/CHR/BAPM and other event tags. Gate test verifies
event-type distribution round-trip.

### ~~2. Family Events Not Exported to FAM Nodes~~ — FIXED

`family_to_fam_node` now accepts `&[Event]` and `&[Place]` and emits MARR/DIV
and other family event tags.

### ~~5. Repository (REPO) Records Not Handled~~ — FIXED

REPO records are imported and exported. Gate test verifies repository count
round-trip.

### ~~3. Inline Citation Round-Trip~~ — FIXED

Inline `SOUR` citations (`2 SOUR @Sx@` within INDI/FAM event subrecords) now
round-trip correctly. Import maps citations into `Citation` entities with
`CitationRef` linkages, and export re-emits inline `SOUR` subtrees on event
records. Synthetic end-to-end coverage verifies `PAGE`, `QUAY`, and `DATA/TEXT`
mapping plus citation count preservation across import → export → re-import.

### ~~6. Phase 1B GEDCOM Corpus Hardening~~ — IN PROGRESS (baseline in place)

`crates/gedcom/tests/corpus_roundtrip_test.rs` now runs import/export/re-import
against five vendor fixtures (Ancestry, RootsMagic, Gramps, Legacy, PAF),
checks round-trip row and assertion-distribution stability, and validates that
standard deferred-tag counters are present for `ASSO`, `CHAN`, `DATA`, `NOTE`,
and `OBJE`.

### ~~7. torture551 Standard-Tag Accounting Incomplete~~ — FIXED

Follow-up bead `rustygene-de3` is now addressed. Recognized GEDCOM 5.5.1
standard tags are explicitly counted as deferred (instead of unhandled) when
they appear in edge-case paths, and `crates/gedcom/tests/torture551_tag_accounting_test.rs`
now enforces zero unhandled standard tags for `torture551.ged`.

### ~~16. Vendor custom ID tags were raw-only~~ — PARTIALLY FIXED

Known vendor ID tags (`_APID`, `_FSID`, `_HPID`, `_PID`, `_WPID`, `_LKID`) are
now mapped to explicit `external_reference` assertions on Person/Family
entities during import, while still preserving raw subtrees for round-trip
fidelity.

### ~~8. Inline NOTE links were raw-only~~ — FIXED

- **Resolved by:** `rustygene-gwx`

Inline `NOTE @N...@` links and embedded NOTE text now generate typed
`note_ref` and `note` assertions on owning entities (Person/Family/Event)
while preserving existing raw GEDCOM subtree round-trip behavior.

### ~~10. ASSO/ASSOC records were raw-only~~ — FIXED

- **Resolved by:** `rustygene-0wf`

`ASSO`/`ASSOC` links are now surfaced as typed `association` assertions on
Person entities (including `RELA` and NOTE/source-presence metadata), while
the original GEDCOM subtrees continue to round-trip via raw preservation and
re-emission.

### ~~17. Vendor custom metadata tags remained unnormalized~~ — FIXED

- **Resolved by:** `rustygene-x22`

Metadata-oriented vendor tags (including `_MSER`, `_OID`, `_ATL`, `_ORIG`,
`_DATE`, and related custom metadata tags) now map to typed
`vendor_metadata` assertions during import for Person/Family/Repository/Source/
Media/Note entities while preserving raw GEDCOM subtrees for export parity.

Corpus round-trip coverage now includes `testdata/gedcom/vendor_metadata_sample.ged`
to verify both typed assertion generation and metadata tag re-emission.

---

## Open Gaps

### 9. Multimedia (OBJE) Coverage is Root-Level Only

- **Impact:** LOW · Phase 1B

Root-level `OBJE` records are imported/exported as typed `Media` entities.
Inline `OBJE` links on INDI/FAM are now mapped to `media_ref` assertions.
Remaining gap is broader typed link parity across all owner contexts (notably
event-level OBJE coverage and richer link metadata parity).

### ~~15. torture551.ged Round-Trip Citation Drift~~ — FIXED

- **Impact:** MEDIUM · Phase 1B Corpus Hardening · **Bead: rustygene-p0k**

`torture551.ged` citation round-trip drift is resolved.

- The full diagnostic `corpus_roundtrip_torture551_ged_diagnostic` is active and passing.
- Citation-bearing SOUR contexts (including complex torture551 paths) now preserve
  row counts and assertion distribution across import → export → re-import.

**Validation:** `corpus_roundtrip_torture551_event_count_regression` and full
diagnostic round-trip tests both pass.

---

## Round-Trip Fidelity Gaps

### ~~11. Family link xrefs not preserved~~ — FIXED

- **Resolved by:** `rustygene-9el`

`family_to_fam_node` now prefers preserved original GEDCOM person xrefs when
emitting `HUSB` / `WIFE` / `CHIL` links. Regression coverage includes both:

- import → export validation that family links retain original person xrefs
- deterministic fallback to `@I<entity_uuid_simple>@` when no original xref exists

### ~~12. CHAN (Change Timestamp) Not Exported~~ — FIXED

`CHAN` subtrees are now explicitly preserved during import and re-emitted on
export via raw GEDCOM subtree carryover. Regression tests verify DATE/TIME
round-trip preservation.

### ~~13. HEAD Block Incomplete on Export~~ — FIXED

Exported HEAD now includes `GEDC/FORM LINEAGE-LINKED`, `DATE`, `TIME`, and
`LANG` in addition to required `SOUR`, `GEDC/VERS`, and `CHAR UTF-8` fields.

### ~~14. Name TYPE Annotation Export Incomplete~~ — FIXED

`PersonName.name_type` is now re-emitted as `2 TYPE` under `NAME` for
non-default values (e.g., AKA, MARRIED, custom), with regression coverage.

---

## Resolved Issues

The following issues were discovered and fixed during Phase 8.2:

| Issue | Fix |
| --- | --- |
| `DateValue::Textual` serde serialization failure | Changed to struct variant `Textual { value: String }` |
| Family/Relationship table collision on export | `load_family_entities()` filters by `relationship_type IS NULL` |
| Snapshot rebuild "Invalid column type Integer" | `CAST(value AS TEXT)` in assertions snapshot query |
| ISO-8859-1 / Latin-1 encoded GEDCOM files crash import | Latin-1 byte→char fallback in `read_gedcom_file()` |

---

## Confirmed Working

- UTF-8 and Latin-1 (ISO-8859-1 / ANSI) GEDCOM file encoding detection
- Person name import/export: given names, surnames, prefix, suffix, format as `Given /Surname/`
- SEX tag import/export (M/F/custom)
- Family structure: HUSB/WIFE/CHIL links with PEDI lineage type
- FAMS/FAMC back-links rebuilt correctly on export from family data
- Source TITL, AUTH, PUBL fields
- Custom tag pass-through via `_raw_gedcom` (unknown tags survive round-trip for persons)
- Phase 1B corpus hardening baseline: import/export/re-import test across 5 vendor fixtures
- UUID-based entity primary keys
- All three test files import and export without errors or panics:
  - `kennedy.ged` (70 persons, 23 families, 23 events, 474 assertions)
  - `simpsons.ged` (11 persons, 3 families, 3 events, 41 assertions)
  - `torture551.ged` (15 persons, 7 families, 352 assertions)

---

## Remaining Phase 1A Work Items

No remaining open Phase 1A blockers. The previously tracked items (`skt`,
`um3`, `dri`, and inline citation round-trip in `dy8`) are now implemented and
covered by tests.

## Resolved Phase 1A Work Items

Previously tracked as gaps, now implemented:

| Item | Old Bead | Resolution |
| --- | --- | --- |
| Person event import (BIRT, DEAT, etc.) | `rustygene-46m` | Events parsed from INDI records into Event entities |
| Event export (person + family) | `rustygene-ed8` | `person_to_indi_node_with_policy` and `family_to_fam_node` now accept and emit event subrecords |

## Phase 2+ Work Items

The following improvements are deferred to a later phase:

1. **ASSO record import**: Store witness/association links.
2. **xref alias table**: Optionally preserve original xref IDs across
   import/export.
3. **Storage integration tests** (bead `rustygene-41z`): Cover Place, Note,
   Media, and LDS entity CRUD paths.
4. **CLI show/query expansion** (bead `rustygene-c7h`): Add commands for
   Source, Citation, Repository, Note, and Media entities.
5. **HEAD metadata parity**: Preserve/import/export submitter (`SUBM`) metadata
   when represented in domain models.
