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
