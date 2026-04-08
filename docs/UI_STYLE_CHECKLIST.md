# UI Style Checklist (Issue rustygene-3th.11)

## Pages covered

- `app/src/routes/persons/+page.svelte`
- `app/src/routes/families/+page.svelte`
- `app/src/routes/events/+page.svelte`
- `app/src/routes/charts/fan/+page.svelte`
- `app/src/routes/charts/pedigree/+page.svelte`
- `app/src/routes/charts/graph/+page.svelte`

## Checklist

- [x] Primary actions visually distinct from secondary/danger actions (`btn-primary`, `btn-secondary`, `btn-danger`).
- [x] Desktop table scanability improved with compact spacing (`table-compact`).
- [x] Sort/filter controls remain keyboard accessible.
- [x] Chart controls keep secondary actions visually low-emphasis (`ghost`/secondary styles).
- [x] Tooltip fallback is present where labels are truncated or filtered.
