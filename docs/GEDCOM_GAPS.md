# GEDCOM 5.5.1 Import/Export Gaps

Known limitations in GEDCOM handling, discovered during Phase 1A testing with
`testdata/gedcom/kennedy.ged`, `simpsons.ged`, and `torture551.ged`.

Last reviewed: 2026-04-01.

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

---

## Open Gaps

### 3. Inline Citation Round-Trip (Phase 1B)

**Impact: MEDIUM** · Bead: rustygene-dy8

The import code for inline SOUR citations (`2 SOUR @Sx@` within INDI/FAM event
subrecords → Citation + CitationRef) exists and unit tests pass. However:

- **kennedy.ged has zero inline SOUR citations.** Its `0 @Sx@ SOUR` records are
  top-level Sources, not inline citations. The `2 SOUR 11` on line 22 is a HEAD
  stats counter. No test corpus file currently exercises the inline citation
  import path end-to-end.
- **GEDCOM exporter does not emit inline SOUR references** back into INDI/FAM
  subrecords, so citation round-trip would fail even if import worked.
- **torture551.ged** has inline citations but uses CR-only line endings + ANSEL
  encoding, which prevents direct `include_str!` usage.

**Resolution path:** Create a small synthetic GEDCOM test fixture with inline
`2 SOUR @S1@` + PAGE/QUAY sub-nodes. Verify citation import AND export
round-trip with that fixture. This is Phase 1B work (spec §16.1 sub-step 4.5
says "inline source citations" but the primary gate test corpus lacks them).

### 4. NOTE Records Not Stored

**Impact: LOW** · Phase 1B

Stand-alone `NOTE @N1@` records and inline `1 NOTE` subrecords are absorbed by
the raw GEDCOM fallback. They survive round-trip via `_raw_gedcom` but are not
typed entities.

### 6. Multimedia (OBJE) Records Not Handled

**Impact: LOW** · Phase 1B

`OBJE` root-level records are not imported as typed Media entities.

### 7. ASSO (Association) Records Ignored

**Impact: LOW** · Phase 1B+

`1 ASSO @I1@` association records are not parsed or stored.

---

## Round-Trip Fidelity Gaps

### 8. xref IDs Not Preserved

**Impact: MEDIUM**

Original xref identifiers (`@I23@`, `@F5@`, `@S2@`) are not preserved across
import/export. The exporter assigns sequential UUIDs as xrefs (`@I<uuid>@`),
which breaks any external cross-references that relied on the original IDs.
This is by design (UUID-based primary keys), but it is a fidelity loss.

### 9. CHAN (Change Timestamp) Not Exported

**Impact: LOW**

`1 CHAN` subrecords recording the date/time of last modification are parsed
during import but no audit timestamp field is stored on entity types. They are
not re-emitted on export.

### 10. HEAD Block Incomplete on Export

**Impact: LOW**

The exported HEAD record omits several standard fields present in valid GEDCOM
5.5.1 files:
- `1 SUBM @SUBM@` — submitter cross-reference
- `1 DATE <export-date>` — file creation timestamp
- `2 TIME <hh:mm:ss>`
- `1 GEDC / 2 FORM LINEAGE-LINKED` — explicitly declares the GEDCOM form
- `1 LANG <language>` — language of data

### 11. Name TYPE Annotation Not Parsed or Exported

**Impact: LOW**

`2 TYPE Birth` (or `Married`, `Also Known As`, etc.) name-type annotations are
**not parsed** during import — `parse_name_node` hardcodes `name_type:
NameType::Birth` and the `TYPE` tag falls through the `_ => {}` arm. The
exporter also does not emit a `TYPE` subnode.

---

## Resolved Issues

The following issues were discovered and fixed during Phase 8.2:

| Issue | Fix |
|---|---|
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
- UUID-based entity primary keys
- All three test files import and export without errors or panics:
  - `kennedy.ged` (70 persons, 23 families, 23 events, 474 assertions)
  - `simpsons.ged` (11 persons, 3 families, 3 events, 41 assertions)
  - `torture551.ged` (15 persons, 7 families, 352 assertions)

---

## Remaining Phase 1A Work Items

The following items are required by `INITIAL_SPEC.md` Steps 4.3, 5.1, and 5.5
and must be completed before Phase 1A can be closed. Each has a tracking bead.

1. **Replace forbidden `_ => {}` catch-all patterns** (bead `rustygene-skt`, P0):
   4 locations in `crates/gedcom/src/lib.rs` silently drop standard GEDCOM tags.
   Must be replaced with explicit handling, deferred-tag counters, or unknown-tag
   counters.
2. **Citation round-trip** (bead `rustygene-dy8`, P1): Resolve `SOUR` references
   within event subrecords to Citation entities. kennedy.ged imports 0 citations
   despite having `SOUR` references.
3. **Map PLAC tags to Place entities** (bead `rustygene-um3`, P1): Place strings
   are stored inline on events but not mapped to `Place` domain entities.
4. **Gate test fidelity** (bead `rustygene-dri`, P0, blocked by skt + dy8):
   Update e2e_gate_test.rs to compare full assertion graphs per-entity-type and
   per-field, not just given names or total counts.

## Resolved Phase 1A Work Items

Previously tracked as gaps, now implemented:

| Item | Old Bead | Resolution |
|---|---|---|
| Person event import (BIRT, DEAT, etc.) | `rustygene-46m` | Events parsed from INDI records into Event entities |
| Event export (person + family) | `rustygene-ed8` | `person_to_indi_node_with_policy` and `family_to_fam_node` now accept and emit event subrecords |

## Phase 2+ Work Items

The following improvements are deferred to a later phase:

1. **NOTE/REPO/OBJE entity handling**: Model as first-class entity types.
2. **ASSO record import**: Store witness/association links.
3. **xref alias table**: Optionally preserve original xref IDs across
   import/export.
4. **Name type import/export**: Parse `2 TYPE` annotation into `NameType` field.
5. **Storage integration tests** (bead `rustygene-41z`): Cover Place, Note,
   Media, and LDS entity CRUD paths.
6. **CLI show/query expansion** (bead `rustygene-c7h`): Add commands for
   Source, Citation, Repository, Note, and Media entities.
   (currently hardcoded to `Birth`); emit on export for non-birth names.
5. **HEAD block completeness** (bead `rustygene-8mg`): Emit DATE, SUBM,
   GEDC.FORM, LANG on export.
