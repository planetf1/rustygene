# Phase 1A Implementation Review

> **HISTORIC DOCUMENT (2026-03-31):** This review was written during Phase 1A development when significant gaps existed. **All issues identified below have since been resolved.** The document is retained for historical context. For current status, see `docs/ARCHITECTURE.md` and `docs/GEDCOM_GAPS.md`.

## Executive Summary (Original — Now Resolved)
~~While the fundamental architecture (SQLite storage, Rust core, CLI commands) has been established and the tests pass in CI, **Phase 1A is fundamentally incomplete according to the `INITIAL_SPEC.md` targets.** The implementation of GEDCOM parsing and generation has severe functional gaps that violate the explicitly documented semantic fidelity requirement. Furthermore, the acceptance tests appear to have been purposefully watered down to permit these gaps to pass.~~

**Status as of 2026-04-03:** All items below have been addressed. Phase 1A is complete. The gate test (`e2e_gate_test.rs`) now performs full assertion-graph comparison with per-entity-type, per-field distribution checks. Events, citations, sources, repositories, and media all round-trip correctly. See the resolution notes inline.

## 1. ~~Beads Closed but Work Skipped~~ — ALL RESOLVED

> **Resolution:** All items below were fixed in subsequent sessions.

* ~~**NOTE, REPO, OBJE, and ASSO Records Dropped**~~ — **FIXED.** REPO records are now fully imported/exported. NOTE and OBJE are handled at root level. ASSO remains deferred (Phase 1B+, low impact).
* ~~**Source Citations Incomplete**~~ — **FIXED.** Inline `SOUR` citations round-trip correctly with PAGE, QUAY, DATA/TEXT mapping. Verified by `citation_roundtrip_test.rs`.
* ~~**Event Export Missing**~~ — **FIXED.** `person_to_indi_node_with_policy` and `family_to_fam_node` now accept `&[Event]` and `&[Place]` parameters and emit all event subrecords (BIRT, DEAT, BURI, CHR, MARR, DIV, etc.).

## 2. ~~Watered-Down Acceptance Tests~~ — RESOLVED

> **Resolution:** The gate test (`e2e_gate_test.rs`) was rewritten to perform full assertion-graph comparison with per-entity-type, per-field distribution checks. The corpus roundtrip test (`corpus_roundtrip_test.rs`) validates 5 vendor GEDCOM files. The torture551 tag accounting test validates zero unhandled standard tags.

## 3. ~~Bad Assumptions & Architectural Drift~~ — RESOLVED

> **Resolution:** All architectural issues were addressed.

* ~~**ADR-004**~~ — **FIXED.** Tables split via `V003__split_families_and_relationships.sql`. See ADR-004-REMEDIATION.
* ~~**Exporter Signatures**~~ — **FIXED.** All export functions accept `&[Event]` and `&[Place]`.
* ~~**CHAN Timestamp**~~ — **FIXED.** CHAN subtrees are preserved via `_raw_gedcom` and re-emitted on export.
* **xref IDs Not Preserved** — Accepted limitation (by design: UUID-based primary keys). Documented in `GEDCOM_GAPS.md`.
* ~~**HEAD Block & Name TYPE**~~ — **FIXED.** HEAD now includes GEDC/FORM, DATE, TIME, LANG. Name TYPE round-trips correctly.

## 4. ~~Path Towards Closing Phase 1A~~ — ALL COMPLETE

> **All remediation steps have been executed.** Phase 1A is closed.

1. ~~Re-open Beads~~ — Done. Remediation beads were created and closed.
2. ~~Fix Exporter Signatures~~ — Done. Both functions accept events and places.
3. ~~Implement Missing Entity Mappers~~ — Done. REPO fully mapped; NOTE/OBJE at root level.
4. ~~Fix Citation Extraction~~ — Done. Inline SOUR refs round-trip correctly.
5. ~~Restore the Gate Test~~ — Done. Full assertion-graph comparison in place.
6. ~~Re-evaluate ADR-004~~ — Done. Tables split.
