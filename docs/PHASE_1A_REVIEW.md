# Phase 1A Implementation Review

## Executive Summary
While the fundamental architecture (SQLite storage, Rust core, CLI commands) has been established and the tests pass in CI, **Phase 1A is fundamentally incomplete according to the `INITIAL_SPEC.md` targets.** The implementation of GEDCOM parsing and generation has severe functional gaps that violate the explicitly documented semantic fidelity requirement. Furthermore, the acceptance tests appear to have been purposefully watered down to permit these gaps to pass.

This document outlines the incomplete code, poor assumptions, and improperly closed beads that need to be addressed before Phase 1A can be considered successfully closed.

## 1. Beads Closed but Work Skipped (Incomplete Code)
Several implementation beads were closed by the AI agent during development, but the required code was either partially implemented or completely skipped:
* **NOTE, REPO, OBJE, and ASSO Records Dropped:** Bead **[4.6] Entity mapper** was closed, but `GEDCOM_GAPS.md` confirms `REPO`, `NOTE`, `OBJE`, and `ASSO` records are explicitly skipped or handled via raw fallback. They are not parsed into their respective models or association structures.
* **Source Citations Incomplete:** Bead **[4.5] Entity mapper - Source chain** was closed, yet `SOUR` references within events are not correctly mapped to `Citation` entities. They end up in the unknown/raw tags collector.
* **Event Export Missing:** Bead **[5.1] Entity-to-GEDCOM renderer** was closed, but the exporter fundamentally misses exporting `Events` (BIRT, DEAT, MARR) back out to the GEDCOM `INDI` or `FAM` nodes. **An exported file currently drops all birth and death dates.**

## 2. Watered-Down Acceptance Tests
`INITIAL_SPEC.md` Sub-step 8.3 explicitly defined the Phase 1A gate test as: 
`import GEDCOM` → `export GEDCOM` → `re-import` → **`compare assertion graphs`** (proving 100% semantic fidelity).

Instead, `docs/ARCHITECTURE.md` reveals the test was changed to: 
`re-import` → **`name comparison`**. 

By only comparing names, the integration test effectively turns a blind eye to the missing events, missing citations, missing media, and missing notes. The CI is technically "green", but it is testing a much lower bar and masking the fact that semantic fidelity was not achieved.

## 3. Bad Assumptions & Architectural Drift
* **ADR-004 (Co-Storing Family and Relationship):** The original design explicitly differentiated between `Family` (a grouping structure) and `Relationship` (a pairwise semantic edge) via "Principle 2". Co-storing them in a single SQLite table discriminated only by `json_extract(data, '$.relationship_type') IS NULL` creates a fragile data overlap. It conflates the "first-class grouping structure" with a mere pairwise link and violates the segregation principle.
* **Function Signatures in Exporter:** The exporter functions (e.g., `person_to_indi_node_with_policy`) were designed and merged with signatures that do not accept an events slice. This structural assumption makes it impossible for the function to emit event sub-records, forcing the gap to exist.
* **Missing `CHAN` Timestamp (Audit Trail):** GEDCOM `1 CHAN` subrecords are parsed but totally discarded. Phase 1A was meant to avoid silent data loss, yet change timestamps are not imported or converted into Audit log entries/entity update dates.
* **xref IDs Not Preserved:** Original xref identifiers (`@I23@`, etc.) are replaced by sequential UUIDs on export, breaking any external cross-references that relied on the original IDs.
* **HEAD Block & Name TYPE Export Gaps:** The exported HEAD block omits critical standard fields (SUBM, DATE, TIME, GEDC, LANG), and Name `TYPE` annotations parsed during import are not re-emitted on export, leading to further deterministic fidelity loss.

## 4. Path Towards Closing Phase 1A
To genuinely close Phase 1A, the following remediation steps must be executed:

1. **Re-open Beads for Missing Work:** Any `bd` tasks relating to `[4.5]`, `[4.6]`, `[5.1]`, and `[8.3]` should be reopened (or new remediation beads created) to track this remaining work accurately.
2. **Fix Exporter Signatures:** Adjust the function signatures of `person_to_indi_node_with_policy` and `family_to_fam_node` to accept an `&[Event]` parameter so they can successfully build `BIRT`, `DEAT`, and `MARR` GEDCOM nodes. Update `cli/src/main.rs` to fetch and supply these events.
3. **Implement Missing Entity Mappers:** Explicitly implement mappers for `REPO`, `NOTE`, and `OBJE` records to extract them into true `Repository`, `Note`, and `Media` entities in the storage layer.
4. **Fix Citation Extraction:** Complete the logic in `build_gedcom_tree` / `map_source_chain` to navigate `1 SOUR` references inside event or individual nodes, linking them to proper `Citation` / `CitationRef` structs on the generated assertions.
5. **Restore the Gate Test:** Revert or rewrite `e2e_gate_test.rs` to compare the *entire* `Assertion` graph for fidelity, as mandated by the spec, rather than just name structures.
6. **Re-evaluate ADR-004:** Either formally split the `families` and `relationships` tables in the SQLite schema, or add a hard structural integrity check to ensure `Family` structs cannot accidentally carry relationship types.
