# GEDCOM 5.5.1 Import/Export Gaps

Discovered during Phase 8.1/8.2 testing with real-world GEDCOM files:
`testdata/gedcom/kennedy.ged`, `simpsons.ged`, and `torture551.ged`.

---

## Critical Export Gaps

### 1. Person Events Not Exported to INDI Nodes

**Impact: HIGH**

Events (birth, death, burial, christening, etc.) are parsed and stored in the
`events` table during import, but `person_to_indi_node_with_policy` does not
accept or emit any event subrecords. The exported INDI node only contains NAME
and SEX tags.

Missing GEDCOM tags in INDI records:
- `BIRT` (birth: date + place)
- `DEAT` (death: date + place)
- `BURI` (burial)
- `CHR` (christening)
- `BAPM` (baptism)
- `CONF` (confirmation)
- `GRAD` (graduation)
- `OCCU`, `RELI`, `EDUC`, `NATI`, `TITL` (attribute events)
- `NOTE` references attached to persons

**Affected test data:** kennedy.ged shows 238 missing DATE, 83 missing PLAC, 66
missing BIRT, and 41 missing DEAT tags in the exported output.

**Root cause:** `person_to_indi_node_with_policy` signature does not take events
as a parameter. The events are stored with participant person IDs in the events
table, but are never re-attached to the INDI node during export. The fix
requires loading events per person in the CLI export pipeline and passing them
into the export function.

### 2. Family Events Not Exported to FAM Nodes

**Impact: HIGH**

Family events (marriage, divorce, separation, etc.) are parsed during import but
`family_to_fam_node` does not emit any FAM event subrecords.

Missing GEDCOM tags in FAM records:
- `MARR` (marriage: date + place)
- `DIV` (divorce)
- `SEPR`, `CENS`, `EVEN` (other family events)

**Root cause:** Same as person events — `family_to_fam_node` does not accept an
events slice.

---

## Import Gaps

### 3. Source Citation Mapping Incomplete

**Impact: MEDIUM**

SOUR references within event records and individual notes (`1 SOUR @S1@`) are
parsed, but the linkage between citations and assertions is incomplete.
kennedy.ged imports 70 persons, 23 events, 474 assertions, and 239 unknown
tags, but 0 citation entities are created. The `SOUR` reference tracking within
event subrecords is likely going to the unknown-tag collector rather than being
resolved to Source entities.

### 4. NOTE Records Not Stored

**Impact: MEDIUM**

`NOTE` root-level records and inline `1 NOTE` subrecords are not persisted to
any typed entity table. They are absorbed by the custom tag / raw GEDCOM
fallback mechanism, which means they survive round-trip only if attached to a
person record (via `_raw_gedcom`), and only if that person is exported.
Stand-alone `NOTE @N1@ ...` records are lost entirely on export.

### 5. Repository (REPO) Records Not Handled

**Impact: LOW**

`REPO` (repository) root-level records are not imported to any entity type.
They are silently skipped. References from Source records to repositories
(`3 REPO @R1@`) remain as raw/unknown tags.

### 6. Multimedia (OBJE) Records Not Handled

**Impact: LOW**

`OBJE` (object/media) root-level records are not imported. Inline `1 OBJE`
subrecords are captured as raw tags.

### 7. ASSO (Association) Records Ignored

**Impact: LOW**

`1 ASSO @I1@` records linking persons (witness relationships, etc.) are not
parsed or stored.

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
