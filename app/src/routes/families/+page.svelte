<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { api } from '$lib/api';
  import FamilyForm from '$lib/components/FamilyForm.svelte';

  type FamilyRow = {
    id: string;
    partner1?: { id: string; display_name: string };
    partner2?: { id: string; display_name: string };
    children: { id: string; display_name: string }[];
    events: { event_type: string; date: string | null }[];
  };

  type SortField = 'family' | 'marriage_year' | 'children';

  type FamilyListState = {
    searchText: string;
    page: number;
    pageSize: number;
    sortField: SortField;
    sortDir: 'asc' | 'desc';
  };

  const FAMILY_LIST_STATE_KEY = 'rg:list:families:v1';
  const defaultFamilyState: FamilyListState = {
    searchText: '',
    page: 0,
    pageSize: 50,
    sortField: 'family',
    sortDir: 'asc'
  };

  let families: FamilyRow[] = [];
  let total = 0;
  let loading = false;
  let error = '';
  let page = 0;
  let pageSize = 50;
  let sortField: SortField = 'family';
  let sortDir: 'asc' | 'desc' = 'asc';
  let searchText = '';
  let showCreate = false;

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  $: totalPages = Math.max(1, Math.ceil(total / pageSize));
  $: pageStart = total === 0 ? 0 : page * pageSize + 1;
  $: pageEnd = Math.min((page + 1) * pageSize, total);

  function marriageYear(row: FamilyRow): string {
    const marriage = row.events.find((e) => e.event_type.toLowerCase().includes('marriage'));
    if (!marriage?.date) return '—';
    return marriage.date.match(/\d{4}/)?.[0] ?? '—';
  }

  function familyLabel(row: FamilyRow): string {
    const p1 = row.partner1?.display_name ?? 'Unknown';
    const p2 = row.partner2?.display_name ?? 'Unknown';
    return `${p1} + ${p2}`;
  }

  async function loadPage(resetPage = false): Promise<void> {
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

      const result = await api.get<{ total: number; items: FamilyRow[] }>(`/api/v1/families?${query.toString()}`);
      families = result.items;
      total = result.total;
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load families';
    } finally {
      loading = false;
    }
  }

  function onSearchInput(): void {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => void loadPage(true), 300);
  }

  function toggleSort(field: SortField): void {
    if (sortField === field) {
      sortDir = sortDir === 'asc' ? 'desc' : 'asc';
    } else {
      sortField = field;
      sortDir = 'asc';
    }
    void loadPage(true);
  }

  function sortIndicator(field: SortField): string {
    if (sortField !== field) return '';
    return sortDir === 'asc' ? ' ▲' : ' ▼';
  }

  function restoreListState(): void {
    if (typeof window === 'undefined') {
      return;
    }

    const raw = localStorage.getItem(FAMILY_LIST_STATE_KEY);
    if (!raw) {
      return;
    }

    try {
      const state = JSON.parse(raw) as Partial<FamilyListState>;
      searchText = state.searchText ?? defaultFamilyState.searchText;
      page = Number.isInteger(state.page) ? Math.max(0, state.page as number) : defaultFamilyState.page;
      pageSize = [25, 50, 100, 250].includes(state.pageSize ?? -1)
        ? (state.pageSize as number)
        : defaultFamilyState.pageSize;
      sortField = (state.sortField as SortField) ?? defaultFamilyState.sortField;
      sortDir = state.sortDir === 'desc' ? 'desc' : 'asc';
    } catch {
      // ignore malformed saved state
    }
  }

  function persistListState(): void {
    if (typeof window === 'undefined') {
      return;
    }

    const state: FamilyListState = {
      searchText,
      page,
      pageSize,
      sortField,
      sortDir
    };
    localStorage.setItem(FAMILY_LIST_STATE_KEY, JSON.stringify(state));
  }

  function resetListView(): void {
    searchText = defaultFamilyState.searchText;
    page = defaultFamilyState.page;
    pageSize = defaultFamilyState.pageSize;
    sortField = defaultFamilyState.sortField;
    sortDir = defaultFamilyState.sortDir;
    void loadPage(true);
  }

  $: searchText, page, pageSize, sortField, sortDir, persistListState();

  onMount(async () => {
    restoreListState();
    await loadPage();
  });
</script>

<main class="panel">
  <header class="header">
    <h1>Families</h1>
    <button type="button" class="btn-primary" on:click={() => (showCreate = true)}>+ New family</button>
  </header>

  <div class="toolbar">
    <input
      bind:value={searchText}
      on:input={onSearchInput}
      placeholder="Search by partner name…"
      type="search"
      class="search-input"
    />
    <label class="page-size-label">
      Show
      <select bind:value={pageSize} on:change={() => loadPage(true)} class="page-size-select">
        <option value={25}>25</option>
        <option value={50}>50</option>
        <option value={100}>100</option>
        <option value={250}>250</option>
      </select>
    </label>
    <button type="button" class="btn-secondary" on:click={resetListView}>Reset view</button>
  </div>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if loading}
    <p class="loading-msg">Loading families…</p>
  {:else}
    <table class="table-compact">
      <thead>
        <tr>
          <th class="sortable" on:click={() => toggleSort('family')}>Family{sortIndicator('family')}</th>
          <th class="sortable" on:click={() => toggleSort('marriage_year')}>Marriage{sortIndicator('marriage_year')}</th>
          <th class="sortable" on:click={() => toggleSort('children')}>Children{sortIndicator('children')}</th>
        </tr>
      </thead>
      <tbody>
        {#if families.length === 0}
          <tr><td colspan="3" class="empty">No families found.</td></tr>
        {:else}
          {#each families as family}
            <tr on:click={() => goto(`/families/${family.id}`)}>
              <td>{familyLabel(family)}</td>
              <td>{marriageYear(family)}</td>
              <td>{family.children.length}</td>
            </tr>
          {/each}
        {/if}
      </tbody>
    </table>

    <div class="pagination">
      <span class="page-info">{total === 0 ? '0' : `${pageStart}–${pageEnd}`} of {total}</span>
      <div class="page-controls">
        <button type="button" class="btn-page" disabled={page === 0} on:click={() => { page = 0; void loadPage(); }}>«</button>
        <button type="button" class="btn-page" disabled={page === 0} on:click={() => { page -= 1; void loadPage(); }}>‹</button>
        <span class="page-num">Page {page + 1} of {totalPages}</span>
        <button type="button" class="btn-page" disabled={page >= totalPages - 1} on:click={() => { page += 1; void loadPage(); }}>›</button>
        <button type="button" class="btn-page" disabled={page >= totalPages - 1} on:click={() => { page = totalPages - 1; void loadPage(); }}>»</button>
      </div>
    </div>
  {/if}
</main>

{#if showCreate}
  <button type="button" class="overlay" aria-label="Close create family panel" on:click={() => (showCreate = false)}></button>
  <aside class="slideover">
    <FamilyForm
      mode="create"
      on:cancel={() => (showCreate = false)}
      on:saved={(event: CustomEvent<{ id: string }>) => {
        showCreate = false;
        void goto(`/families/${event.detail.id}`);
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

  .page-info { color: #6b5fa0; }

  .page-controls {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }

  .page-num { padding: 0 0.5rem; }

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

  .btn-primary:hover { background: #5b21b6; }

  .btn-page {
    background: #f3edff;
    color: #55389a;
    border: 1px solid #dfd2f8;
    border-radius: 0.35rem;
    padding: 0.2rem 0.5rem;
    cursor: pointer;
    font-size: 0.85rem;
  }

  .btn-page:hover:not(:disabled) { background: #ede5ff; }

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
