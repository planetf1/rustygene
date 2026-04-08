<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { api } from '$lib/api';
  import EventForm from '$lib/components/EventForm.svelte';

  type EventRow = {
    id: string;
    event_type: string;
    date: string | null;
    place_id: string | null;
    participants: { person_id: string; role: string }[];
    confidence: number;
  };

  type PersonOption = {
    id: string;
    display_name: string;
  };

  type SortField = 'event_type' | 'date' | 'principal' | 'confidence';
  type SortDirection = 'asc' | 'desc';

  type EventListState = {
    sortField: SortField;
    sortDirection: SortDirection;
    typeFilter: string;
    personFilter: string;
    fromYear: string;
    toYear: string;
    pageSize: number;
  };

  const EVENT_LIST_STATE_KEY = 'rg:list:events:v1';
  const defaultEventState: EventListState = {
    sortField: 'date',
    sortDirection: 'desc',
    typeFilter: '',
    personFilter: '',
    fromYear: '',
    toYear: '',
    pageSize: 50
  };

  let rows: EventRow[] = [];
  let filteredRows: EventRow[] = [];
  let loading = false;
  let loadingMore = false;
  let error = '';
  let offset = 0;
  let pageSize = 50;
  let hasMore = true;
  let showCreate = false;
  let people: PersonOption[] = [];
  const personNameById = new Map<string, string>();

  let sortField: SortField = 'date';
  let sortDirection: SortDirection = 'desc';

  let typeFilter = '';
  let personFilter = '';
  let fromYear = '';
  let toYear = '';

  function restoreListState(): void {
    if (typeof window === 'undefined') {
      return;
    }

    const raw = localStorage.getItem(EVENT_LIST_STATE_KEY);
    if (!raw) {
      return;
    }

    try {
      const state = JSON.parse(raw) as Partial<EventListState>;
      sortField = (state.sortField as SortField) ?? defaultEventState.sortField;
      sortDirection = state.sortDirection === 'asc' ? 'asc' : 'desc';
      typeFilter = state.typeFilter ?? defaultEventState.typeFilter;
      personFilter = state.personFilter ?? defaultEventState.personFilter;
      fromYear = state.fromYear ?? defaultEventState.fromYear;
      toYear = state.toYear ?? defaultEventState.toYear;
      pageSize = [25, 50, 100, 250].includes(state.pageSize ?? -1)
        ? (state.pageSize as number)
        : defaultEventState.pageSize;
    } catch {
      // ignore malformed saved state
    }
  }

  function persistListState(): void {
    if (typeof window === 'undefined') {
      return;
    }

    const state: EventListState = {
      sortField,
      sortDirection,
      typeFilter,
      personFilter,
      fromYear,
      toYear,
      pageSize
    };
    localStorage.setItem(EVENT_LIST_STATE_KEY, JSON.stringify(state));
  }

  function resetListView(): void {
    sortField = defaultEventState.sortField;
    sortDirection = defaultEventState.sortDirection;
    typeFilter = defaultEventState.typeFilter;
    personFilter = defaultEventState.personFilter;
    fromYear = defaultEventState.fromYear;
    toYear = defaultEventState.toYear;
    pageSize = defaultEventState.pageSize;
    void loadPage(true);
  }

  function applyFilters(): void {
    const base = rows.filter((event) => {
      if (typeFilter && !event.event_type.toLowerCase().includes(typeFilter.toLowerCase())) {
        return false;
      }

      if (personFilter && !event.participants.some((participant) => participant.person_id === personFilter)) {
        return false;
      }

      const year = event.date?.match(/\d{4}/)?.[0];
      const numericYear = year ? Number(year) : null;

      if (fromYear && numericYear !== null && numericYear < Number(fromYear)) {
        return false;
      }
      if (toYear && numericYear !== null && numericYear > Number(toYear)) {
        return false;
      }

      return true;
    });

    filteredRows = [...base].sort((left, right) => compareRows(left, right));
  }

  function personName(personId: string): string {
    return personNameById.get(personId) ?? personId;
  }

  function principalName(event: EventRow): string {
    const principal = event.participants.find((participant) => participant.role.toLowerCase() === 'principal')
      ?? event.participants[0];
    return principal ? personName(principal.person_id) : '—';
  }

  function compareRows(left: EventRow, right: EventRow): number {
    const direction = sortDirection === 'asc' ? 1 : -1;

    switch (sortField) {
      case 'event_type':
        return left.event_type.localeCompare(right.event_type) * direction;
      case 'principal':
        return principalName(left).localeCompare(principalName(right)) * direction;
      case 'confidence':
        return (left.confidence - right.confidence) * direction;
      case 'date':
      default: {
        const l = left.date ?? '';
        const r = right.date ?? '';
        return l.localeCompare(r) * direction;
      }
    }
  }

  function toggleSort(field: SortField): void {
    if (sortField === field) {
      sortDirection = sortDirection === 'asc' ? 'desc' : 'asc';
    } else {
      sortField = field;
      sortDirection = field === 'date' || field === 'confidence' ? 'desc' : 'asc';
    }
    applyFilters();
  }

  function sortIndicator(field: SortField): string {
    if (sortField !== field) {
      return '↕';
    }
    return sortDirection === 'asc' ? '↑' : '↓';
  }

  function onPageSizeChange(): void {
    void loadPage(true);
  }

  async function loadPeople(): Promise<void> {
    people = await api.get<PersonOption[]>('/api/v1/persons?limit=500&offset=0');
    personNameById.clear();
    for (const row of people) {
      personNameById.set(row.id, row.display_name);
    }
  }

  async function loadPage(reset = false): Promise<void> {
    const nextOffset = reset ? 0 : offset;
    if (reset) {
      loading = true;
      error = '';
    } else {
      loadingMore = true;
    }

    try {
      const query = new URLSearchParams({
        limit: String(pageSize),
        offset: String(nextOffset)
      });

      const result = await api.get<EventRow[]>(`/api/v1/events?${query.toString()}`);
      rows = reset ? result : [...rows, ...result];
      offset = nextOffset + result.length;
      hasMore = result.length === pageSize;
      applyFilters();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load events';
    } finally {
      loading = false;
      loadingMore = false;
    }
  }

  $: typeFilter, personFilter, fromYear, toYear, applyFilters();
  $: sortField, sortDirection, typeFilter, personFilter, fromYear, toYear, pageSize, persistListState();

  onMount(async () => {
    restoreListState();
    await loadPeople();
    await loadPage(true);
  });
</script>

<main class="panel">
  <header class="header">
    <h1>Events</h1>
    <button type="button" class="btn-primary" on:click={() => (showCreate = true)}>New event</button>
  </header>

  <div class="filters">
    <label>
      Type
      <input bind:value={typeFilter} placeholder="Birth, Census, Marriage…" />
    </label>
    <label>
      Person
      <select bind:value={personFilter}>
        <option value="">All people</option>
        {#each people as person}
          <option value={person.id}>{person.display_name}</option>
        {/each}
      </select>
    </label>
    <label>
      From year
      <input bind:value={fromYear} placeholder="e.g. 1800" />
    </label>
    <label>
      To year
      <input bind:value={toYear} placeholder="e.g. 1900" />
    </label>
    <label>
      Page size
      <select bind:value={pageSize} on:change={onPageSizeChange}>
        <option value={25}>25</option>
        <option value={50}>50</option>
        <option value={100}>100</option>
        <option value={250}>250</option>
      </select>
    </label>
    <div class="filters-actions">
      <button type="button" class="btn-secondary" on:click={resetListView}>Reset view</button>
    </div>
  </div>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if loading}
    <p>Loading events…</p>
  {:else}
    <table class="table-compact">
      <thead>
        <tr>
          <th><button type="button" class="sort-head" on:click={() => toggleSort('event_type')}>Type {sortIndicator('event_type')}</button></th>
          <th><button type="button" class="sort-head" on:click={() => toggleSort('date')}>Date {sortIndicator('date')}</button></th>
          <th><button type="button" class="sort-head" on:click={() => toggleSort('principal')}>Principal {sortIndicator('principal')}</button></th>
          <th>Participants</th>
          <th><button type="button" class="sort-head" on:click={() => toggleSort('confidence')}>Confidence {sortIndicator('confidence')}</button></th>
        </tr>
      </thead>
      <tbody>
        {#if filteredRows.length === 0}
          <tr>
            <td colspan="5">No events found.</td>
          </tr>
        {:else}
          {#each filteredRows as event}
            <tr on:click={() => goto(`/events/${event.id}`)}>
              <td>{event.event_type}</td>
              <td>{event.date ?? '—'}</td>
              <td>{principalName(event)}</td>
              <td>{event.participants.length}</td>
              <td>{event.confidence.toFixed(2)}</td>
            </tr>
          {/each}
        {/if}
      </tbody>
    </table>

    {#if hasMore}
      <button type="button" class="btn-secondary load-more" disabled={loadingMore} on:click={() => loadPage(false)}>
        {loadingMore ? 'Loading…' : 'Load more'}
      </button>
    {/if}
  {/if}
</main>

{#if showCreate}
  <button type="button" class="overlay" aria-label="Close create event panel" on:click={() => (showCreate = false)}></button>
  <aside class="slideover">
    <EventForm
      mode="create"
      on:cancel={() => (showCreate = false)}
      on:saved={(event: CustomEvent<{ id: string }>) => {
        showCreate = false;
        void goto(`/events/${event.detail.id}`);
      }}
    />
  </aside>
{/if}

<style>
  .panel {
    background: linear-gradient(180deg, #ffffff 0%, #fff9ff 100%);
    border: 1px solid var(--rg-border, #e8def8);
    border-radius: 1rem;
    padding: 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 0.95rem;
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .header h1 {
    margin: 0;
  }

  .filters {
    display: grid;
    grid-template-columns: repeat(6, minmax(0, 1fr));
    gap: 0.6rem;
  }

  .filters-actions {
    display: flex;
    align-items: end;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.85rem;
  }

  input {
    border: 1px solid #dfd2f8;
    border-radius: 0.7rem;
    padding: 0.5rem 0.6rem;
    background: #ffffff;
    font: inherit;
  }

  select {
    border: 1px solid #dfd2f8;
    border-radius: 0.7rem;
    padding: 0.5rem 0.6rem;
    background: #ffffff;
    font: inherit;
  }

  table {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
    border: 1px solid var(--rg-border, #e8def8);
    border-radius: 0.8rem;
    overflow: hidden;
  }

  th,
  td {
    background: #ffffff;
  }

  thead th {
    background: linear-gradient(180deg, #f9f2ff 0%, #fff1f9 100%);
  }

  .sort-head {
    border: 0;
    background: transparent;
    padding: 0;
    color: inherit;
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  th,
  td {
    text-align: left;
    padding: 0.55rem 0.65rem;
    border-bottom: 1px solid #f0e8ff;
  }

  tr {
    cursor: pointer;
  }

  tr:hover {
    background: #fdf7ff;
  }

  .btn-primary {
    background: #2563eb;
    color: #fff;
    border: 0;
    border-radius: 0.45rem;
    padding: 0.45rem 0.7rem;
    cursor: pointer;
    width: fit-content;
  }

  .btn-secondary {
    background: #ffffff;
    color: #334155;
    border: 1px solid #cbd5e1;
  }

  .load-more {
    align-self: flex-start;
  }

  .overlay {
    position: fixed;
    inset: 0;
    background: rgb(15 23 42 / 35%);
    border: 0;
    width: 100%;
    padding: 0;
    border-radius: 0;
  }

  .slideover {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(760px, 100%);
    background: #ffffff;
    border-left: 1px solid #e2e8f0;
    padding: 1rem;
    overflow: auto;
  }

  .error {
    color: #b91c1c;
    margin: 0;
  }
</style>
