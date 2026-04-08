# UX Quality Gates (Issue `rustygene-3th.12`)

These gates define measurable pass/fail workflows for core genealogy tasks.

## How to use

- Every UI PR touching core pages **must** list impacted scenario IDs from this document.
- Release candidates must run the **UI regression checklist** in this document before release sign-off.

## Scenario gates

### UX-01 — Find person from list search
- **Area:** `persons/+page.svelte`
- **Steps:** Open Persons page, search by name fragment, open matching row.
- **Pass:** Matching row appears in first page after debounce; opening row navigates to person detail.
- **Fail:** Search returns stale rows, pagination desyncs, or row click fails navigation.

### UX-02 — Sort and page persons list
- **Area:** `persons/+page.svelte`
- **Steps:** Toggle sort on name/birth/death/assertions; move across pages; change page size.
- **Pass:** Sort direction indicator matches data order; page controls keep deterministic range display.
- **Fail:** Inconsistent order across pages, incorrect total/range, or disabled controls behave incorrectly.

### UX-03 — Inspect evidence from detail pages
- **Area:** person/event/family/source/citation detail pages
- **Steps:** Open entity detail, hover evidence chip, open citation/source route, return via context link.
- **Pass:** Evidence preview renders, click-through routes load expected detail, back-context preserves origin.
- **Fail:** Missing preview, broken route transition, or lost navigation context.

### UX-04 — Quick-edit core fields in place
- **Area:** person/event/family detail pages
- **Steps:** Open inline edit, change values, save, then cancel on a second edit attempt.
- **Pass:** Save persists and rerenders updated value; cancel restores previous value without dirty writes.
- **Fail:** Stale UI after save, partial writes, or cancel mutates persisted data.

### UX-05 — Traverse between related records
- **Area:** breadcrumb + related-records graph in detail pages
- **Steps:** Navigate person → related family/event/source, then use breadcrumb path.
- **Pass:** Breadcrumb labels and links reflect traversal context; related graph links target correct records.
- **Fail:** Broken crumb links, incorrect labels, or unrelated node routing.

### UX-06 — Relationship graph semantic controls
- **Area:** `charts/graph/+page.svelte`
- **Steps:** Use semantic default layout, toggle edge-type filters, reset defaults.
- **Pass:** Root-centered layout is stable; legend visible; hidden-edge count updates with filter/collapse state.
- **Fail:** Legend mismatch, reset not restoring defaults, or unreadable root neighborhood.

### UX-07 — Pedigree compact density mode
- **Area:** `charts/pedigree/+page.svelte`
- **Steps:** Toggle standard/compact density, fit content, inspect truncated labels via hover.
- **Pass:** Compact increases visible nodes per viewport, no overlap regressions, tooltip reveals full detail.
- **Fail:** Overlap obscures labels, density toggle no-op, or tooltip missing for truncation.

### UX-08 — Fan chart arc-attached labels
- **Area:** `charts/fan/+page.svelte`
- **Steps:** Render 4–8 generations, verify labels stay attached to arc bands, inspect small segments via tooltip.
- **Pass:** No detached/orphan labels at default zoom; threshold rules suppress unreadable labels; tooltip fallback works.
- **Fail:** Detached labels, unreadable clusters, or missing fallback details.

### UX-09 — Event list scanability
- **Area:** `events/+page.svelte`
- **Steps:** Filter by type/person/year, sort by columns, load additional rows.
- **Pass:** Compact row rhythm remains readable at desktop width; primary/secondary actions are visually distinct.
- **Fail:** Excessive whitespace, ambiguous action hierarchy, or filter/sort inconsistency.

### UX-10 — Action hierarchy consistency
- **Area:** core list/detail/charts pages
- **Steps:** Inspect page-level action bars and destructive controls.
- **Pass:** Primary, secondary, and danger actions are visually distinct and consistent (`btn-primary`, `btn-secondary`, `btn-danger`).
- **Fail:** Equal emphasis across all actions or danger actions not visually escalated.

## UI regression checklist (pre-release)

Run before tagging a release that touches UI:

- [ ] Execute `npm run check` (0 errors).
- [ ] Execute focused regression tests for touched workflows (minimum: `state.test.ts`, `graphMerge.test.ts`, and affected integration tests).
- [ ] Verify UX-01 through UX-10 manually on desktop viewport.
- [ ] Confirm no route-context regressions (`from/back` navigation flows).
- [ ] Confirm chart controls remain keyboard accessible.
- [ ] Capture a short release note listing scenarios validated.
