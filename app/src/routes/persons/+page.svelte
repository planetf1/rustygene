<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { api } from '$lib/api';
  import PersonForm from '$lib/components/PersonForm.svelte';

  type PersonRow = {
    id: string;
    display_name: string;
    birth_year: number | null;
    death_year: number | null;
    assertion_counts: Record<string, number>;
  };

  type SortField = 'name' | 'birth_year' | 'death_year' | 'assertion_count';

  let people: PersonRow[] = [];
  let total = 0;
  let error = '';
  let loading = false;
  let searchText = '';
  let page = 0;
  let pageSize = 50;
  let sortField: SortField = 'name';
  let sortDir: 'asc' | 'desc' = 'asc';
  let showCreatePanel = false;

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  $: totalPages = Math.max(1, Math.ceil(total / pageSize));
  $: pageStart = total === 0 ? 0 : page * pageSize + 1;
  $: pageEnd = Math.min((page + 1) * pageSize, total);

  async function fetchPage(resetPage = false): Promise<void> {
    if (resetPage) page = 0;
    loading = true;
    error = '';

    try {
      const query = new URLSearchParams({
        limit: String(pageSize),
        offset: String(page * pageSize),
        sort: sortField,
        dir: sortDir
      });
      if (searchText.trim()) query.set('q', searchText.trim());

      const result = await api.get<{ total: number; items: PersonRow[] }>(`/api/v1/persons?${query.toString()}`);
      people = result.items;
      total = result.total;
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load persons';
    } finally {
      loading = false;
    }
  }

  function onSearchInput(): void {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => void fetchPage(true), 300);
  }

  function toggleSort(field: SortField): void {
    if (sortField === field) {
      sortDir = sortDir === 'asc' ? 'desc' : 'asc';
    } else {
      sortField = field;
      sortDir = 'asc';
    }
    void fetchPage(true);
  }

  function sortIndicator(field: SortField): string {
    if (sortField !== field) return '';
    return sortDir === 'asc' ? ' ▲' : ' ▼';
  }

  function assertionCount(row: PersonRow): number {
    return Object.values(row.assertion_counts).reduce((sum, v) => sum + v, 0);
  }

  function onPageSizeChange(): void {
    void fetchPage(true);
  }

  onMount(async () => { await fetchPage(); });
</script>

<main class="panel">
  <header class="header">
    <h1>Persons</h1>
    <button type="button" class="btn-primary" on:click={() => (showCreatePanel = true)}>+ New Person</button>
  </header>

  <div class="toolbar">
    <input
      bind:value={searchText}
      on:input={onSearchInput}
      placeholder="Search by name…"
      type="search"
      class="search-input"
    />
    <label class="page-size-label">
      Show
      <select bind:value={pageSize} on:change={onPageSizeChange} class="page-size-select">
        <option value={25}>25</option>
        <option value={50}>50</option>
        <option value={100}>100</option>
        <option value={250}>250</option>
      </select>
    </label>
  </div>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if loading}
    <p class="loading-msg">Loading persons…</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th class="sortable" on:click={() => toggleSort('name')}>
            Display name{sortIndicator('name')}
          </th>
          <th class="sortable" on:click={() => toggleSort('birth_year')}>
            Birth{sortIndicator('birth_year')}
          </th>
          <th class="sortable" on:click={() => toggleSort('death_year')}>
            Death{sortIndicator('death_year')}
          </th>
          <th class="sortable" on:click={() => toggleSort('assertion_count')}>
            Assertions{sortIndicator('assertion_count')}
          </th>
        </tr>
      </thead>
      <tbody>
        {#if people.length === 0}
          <tr><td colspan="4" class="empty">No persons found.</td></tr>
        {:else}
          {#each people as person}
            <tr on:click={() => goto(`/persons/${person.id}`)}>
              <td>{person.display_name}</td>
              <td>{person.birth_year ?? '—'}</td>
              <td>{person.death_year ?? '—'}</td>
              <td>{assertionCount(person)}</td>
            </tr>
          {/each}
        {/if}
      </tbody>
    </table>

    <div class="pagination">
      <span class="page-info">{total === 0 ? '0' : `${pageStart}–${pageEnd}`} of {total}</span>
      <div class="page-controls">
        <button type="button" class="btn-page" disabled={page === 0} on:click={() => { page = 0; void fetchPage(); }}>«</button>
        <button type="button" class="btn-page" disabled={page === 0} on:click={() => { page -= 1; void fetchPage(); }}>‹</button>
        <span class="page-num">Page {page + 1} of {totalPages}</span>
        <button type="button" class="btn-page" disabled={page >= totalPages - 1} on:click={() => { page += 1; void fetchPage(); }}>›</button>
        <button type="button" class="btn-page" disabled={page >= totalPages - 1} on:click={() => { page = totalPages - 1; void fetchPage(); }}>»</button>
      </div>
    </div>
  {/if}
</main>

{#if showCreatePanel}
  <button type="button" class="overlay" aria-label="Close person create panel" on:click={() => (showCreatePanel = false)}></button>
  <aside class="slideover">
    <PersonForm
      mode="create"
      on:cancel={() => (showCreatePanel = false)}
      on:saved={(event: CustomEvent<{ id: string }>) => {
        showCreatePanel = false;
        void goto(`/persons/${event.detail.id}`);
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
    gap: 0.75rem;
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .header h1 {
    margin: 0;
    font-size: 1.25rem;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    flex-wrap: wrap;
  }

  .search-input {
    border: 1px solid #dfd2f8;
    border-radius: 0.5rem;
    padding: 0.4rem 0.6rem;
    font: inherit;
    font-size: 0.9rem;
    flex: 1;
    min-width: 14rem;
    max-width: 26rem;
  }

  .page-size-label {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    font-size: 0.85rem;
    color: #5a4f7d;
    white-space: nowrap;
  }

  .page-size-select {
    border: 1px solid #dfd2f8;
    border-radius: 0.4rem;
    padding: 0.25rem 0.4rem;
    font: inherit;
    font-size: 0.85rem;
    background: #fff;
  }

  table {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
    border: 1px solid var(--rg-border, #e8def8);
    border-radius: 0.75rem;
    overflow: hidden;
    font-size: 0.9rem;
  }

  thead th {
    background: linear-gradient(180deg, #f3edff 0%, #fdf0fb 100%);
    color: #55389a;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }

  th.sortable {
    cursor: pointer;
    user-select: none;
  }

  th.sortable:hover {
    background: #ede5ff;
  }

  th, td {
    text-align: left;
    padding: 0.45rem 0.65rem;
    border-bottom: 1px solid #f0e8ff;
  }

  tbody tr {
    cursor: pointer;
  }

  tbody tr:hover {
    background: #fdf7ff;
  }

  tbody tr:last-child td {
    border-bottom: 0;
  }

  .empty {
    color: #888;
    font-style: italic;
    padding: 1rem;
  }

  .pagination {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 0.85rem;
    color: #5a4f7d;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  .page-info {
    color: #6b5fa0;
  }

  .page-controls {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }

  .page-num {
    padding: 0 0.5rem;
  }

  .btn-primary {
    background: #6d28d9;
    color: #fff;
    border: 0;
    border-radius: 0.45rem;
    padding: 0.4rem 0.75rem;
    cursor: pointer;
    font-size: 0.9rem;
    font-weight: 600;
  }

  .btn-primary:hover {
    background: #5b21b6;
  }

  .btn-page {
    background: #f3edff;
    color: #55389a;
    border: 1px solid #dfd2f8;
    border-radius: 0.35rem;
    padding: 0.2rem 0.5rem;
    cursor: pointer;
    font-size: 0.85rem;
  }

  .btn-page:hover:not(:disabled) {
    background: #ede5ff;
  }

  .btn-page:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .loading-msg {
    color: #888;
    margin: 0;
  }

  .overlay {
    position: fixed;
    inset: 0;
    background: rgb(15 23 42 / 35%);
    border: 0;
    width: 100%;
    padding: 0;
    border-radius: 0;
    cursor: default;
  }

  .slideover {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(760px, 100%);
    background: #ffffff;
    border-left: 1px solid var(--rg-border, #e8def8);
    padding: 1rem;
    overflow: auto;
  }

  .error {
    color: #b91c1c;
    margin: 0;
  }
</style>

