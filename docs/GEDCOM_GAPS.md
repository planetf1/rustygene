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

---

## Open Gaps

### 8. NOTE Records Not Stored

**Impact: LOW** · Phase 1B

Stand-alone `NOTE @N1@` records and inline `1 NOTE` subrecords are absorbed by
the raw GEDCOM fallback. They survive round-trip via `_raw_gedcom` but are not
typed entities.

### 9. Multimedia (OBJE) Coverage is Root-Level Only

**Impact: LOW** · Phase 1B

Root-level `OBJE` records are imported/exported as typed `Media` entities.
Inline `OBJE` links on other records are currently deferred/counted but not yet
mapped into explicit `MediaRef` link structures.

### 10. ASSO (Association) Records Ignored

**Impact: LOW** · Phase 1B+

`1 ASSO @I1@` association records are not parsed or stored.

### 17. Vendor custom metadata tags remain unnormalized

**Impact: LOW** · Phase 2+

Metadata-oriented vendor tags (e.g. `_MSER`, `_OID`, `_ATL`, `_DSCR`, `_ORIG`,
`_DATE`, crop geometry tags) are preserved and accounted for, but currently
remain in `_raw_gedcom` rather than first-class domain fields.

### 15. torture551.ged Round-Trip Event Duplication

**Impact: MEDIUM** · Phase 1B Corpus Hardening · **Bead: rustygene-buh**

`torture551.ged` has documented (torturous) edge cases in GEDCOM structure.
During round-trip testing (import → export → re-import), the event count
increases from 71 to 75 (4 extra events created). This suggests:
- Event references are being duplicated during export (e.g., same event
  referenced in multiple contexts and exported separately)
- Possible interaction with CHAN tags or complex citation/association structures
- `simpsons.ged` round-trip works correctly, so the bug is specific to
  torture551's edge cases.

**Workaround:** `torture551.ged` is deferred from the corpus round-trip test
harness until the export logic can be debugged. `corpus_roundtrip_test.rs` includes
a separate diagnostic test `corpus_roundtrip_torture551_ged_diagnostic` for
isolated investigation. `simpsons.ged` can be added to the corpus array without
issue.

---

## Round-Trip Fidelity Gaps

### 11. xref IDs Not Preserved

**Impact: MEDIUM**

Original xref identifiers (`@I23@`, `@F5@`, `@S2@`) are not preserved across
import/export. The exporter assigns sequential UUIDs as xrefs (`@I<uuid>@`),
which breaks any external cross-references that relied on the original IDs.
This is by design (UUID-based primary keys), but it is a fidelity loss.

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
|---|---|---|
| Person event import (BIRT, DEAT, etc.) | `rustygene-46m` | Events parsed from INDI records into Event entities |
| Event export (person + family) | `rustygene-ed8` | `person_to_indi_node_with_policy` and `family_to_fam_node` now accept and emit event subrecords |

## Phase 2+ Work Items

The following improvements are deferred to a later phase:

1. **ASSO record import**: Store witness/association links.
2. **xref alias table**: Optionally preserve original xref IDs across
   import/export.
4. **Storage integration tests** (bead `rustygene-41z`): Cover Place, Note,
   Media, and LDS entity CRUD paths.
5. **CLI show/query expansion** (bead `rustygene-c7h`): Add commands for
   Source, Citation, Repository, Note, and Media entities.
6. **HEAD metadata parity**: Preserve/import/export submitter (`SUBM`) metadata
   when represented in domain models.
