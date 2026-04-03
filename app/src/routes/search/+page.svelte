<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { api } from '$lib/api';

  type SearchStrategyUi = 'combined' | 'exact' | 'phonetic' | 'fts';
  type EntityTypeUi = 'all' | 'person' | 'family' | 'event' | 'source' | 'note';

  type SearchResult = {
    entity_type: string;
    entity_id: string;
    display_name: string;
    match_fields: string[];
    score: number;
    snippet: string | null;
  };

  type SearchResponse = {
    query: string;
    strategy_used: string;
    results: SearchResult[];
    total: number;
  };

  let q = '';
  let strategy: SearchStrategyUi = 'combined';
  let entityType: EntityTypeUi = 'all';
  let dateFrom = '';
  let dateTo = '';
  let place = '';

  let loading = false;
  let error = '';
  let results: SearchResult[] = [];
  let total = 0;
  let strategyUsed = 'combined';
  let offset = 0;
  const pageSize = 20;

  let lastSyncedUrlQuery = '';

  const strategyOptions: Array<{ value: SearchStrategyUi; label: string }> = [
    { value: 'combined', label: 'Auto' },
    { value: 'exact', label: 'Exact' },
    { value: 'phonetic', label: 'Phonetic' },
    { value: 'fts', label: 'FTS' }
  ];

  const entityTabs: Array<{ value: EntityTypeUi; label: string }> = [
    { value: 'all', label: 'All' },
    { value: 'person', label: 'People' },
    { value: 'family', label: 'Families' },
    { value: 'event', label: 'Events' },
    { value: 'source', label: 'Sources' },
    { value: 'note', label: 'Notes' }
  ];

  function parseUrlFilters(): void {
    const params = $page.url.searchParams;
    q = params.get('q') ?? '';

    const nextStrategy = (params.get('strategy') ?? 'combined').toLowerCase();
    strategy = (['combined', 'exact', 'phonetic', 'fts'].includes(nextStrategy)
      ? nextStrategy
      : 'combined') as SearchStrategyUi;

    const nextType = (params.get('type') ?? 'all').toLowerCase();
    entityType = (['all', 'person', 'family', 'event', 'source', 'note'].includes(nextType)
      ? nextType
      : 'all') as EntityTypeUi;

    dateFrom = params.get('date_from') ?? '';
    dateTo = params.get('date_to') ?? '';
    place = params.get('place') ?? '';
  }

  function buildFilterParams(nextOffset = 0): URLSearchParams {
    const params = new URLSearchParams();
    const trimmedQuery = q.trim();
    if (trimmedQuery) {
      params.set('q', trimmedQuery);
    }

    if (strategy !== 'combined') {
      params.set('strategy', strategy);
    }
    if (entityType !== 'all') {
      params.set('type', entityType);
    }
    if (dateFrom.trim()) {
      params.set('date_from', dateFrom.trim());
    }
    if (dateTo.trim()) {
      params.set('date_to', dateTo.trim());
    }
    if (place.trim()) {
      params.set('place', place.trim());
    }

    params.set('limit', String(pageSize));
    params.set('offset', String(nextOffset));
    return params;
  }

  async function syncUrlAndSearch(reset = true): Promise<void> {
    const nextOffset = reset ? 0 : offset;
    const params = buildFilterParams(nextOffset);

    const urlParams = new URLSearchParams(params);
    urlParams.delete('limit');
    urlParams.delete('offset');
    const filterOnlyQuery = urlParams.toString();

    const nextUrl = filterOnlyQuery ? `/search?${filterOnlyQuery}` : '/search';
    if (lastSyncedUrlQuery !== filterOnlyQuery) {
      lastSyncedUrlQuery = filterOnlyQuery;
      await goto(nextUrl, { keepFocus: true, noScroll: true });
    }

    await runSearch(reset);
  }

  async function runSearch(reset = true): Promise<void> {
    const trimmed = q.trim();
    if (!trimmed) {
      results = [];
      total = 0;
      error = '';
      offset = 0;
      strategyUsed = strategy;
      return;
    }

    loading = true;
    error = '';

    const nextOffset = reset ? 0 : offset;
    const params = buildFilterParams(nextOffset);

    try {
      const response = await api.get<SearchResponse>(`/api/v1/search?${params.toString()}`);
      strategyUsed = response.strategy_used;
      total = response.total;
      offset = nextOffset + response.results.length;
      results = reset ? response.results : [...results, ...response.results];
    } catch (err) {
      error = err instanceof Error ? err.message : 'Search request failed';
      if (reset) {
        results = [];
        total = 0;
      }
    } finally {
      loading = false;
    }
  }

  function scorePercent(score: number): number {
    const clamped = Math.max(0, Math.min(1, score));
    return Math.round(clamped * 100);
  }

  function badgeClass(entity: string): string {
    switch (entity) {
      case 'person':
        return 'person';
      case 'family':
        return 'family';
      case 'event':
        return 'event';
      case 'source':
        return 'source';
      case 'note':
        return 'note';
      default:
        return 'default';
    }
  }

  function labelForEntity(entity: string): string {
    switch (entity) {
      case 'person':
        return 'Person';
      case 'family':
        return 'Family';
      case 'event':
        return 'Event';
      case 'source':
        return 'Source';
      case 'note':
        return 'Note';
      default:
        return entity;
    }
  }

  function detailPathFor(result: SearchResult): string {
    switch (result.entity_type) {
      case 'person':
        return `/persons/${result.entity_id}`;
      case 'family':
        return `/families/${result.entity_id}`;
      case 'event':
        return `/events/${result.entity_id}`;
      case 'source':
        return `/sources/${result.entity_id}`;
      case 'note':
        return `/search?q=${encodeURIComponent(result.entity_id)}`;
      default:
        return '/search';
    }
  }

  function safeSnippetHtml(snippet: string): string {
    const escaped = snippet
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');

    return escaped
      .replace(/&lt;b&gt;/g, '<b>')
      .replace(/&lt;\/b&gt;/g, '</b>');
  }

  function onSubmit(event: SubmitEvent): void {
    event.preventDefault();
    void syncUrlAndSearch(true);
  }

  function setStrategy(next: SearchStrategyUi): void {
    strategy = next;
    void syncUrlAndSearch(true);
  }

  function setEntityType(next: EntityTypeUi): void {
    entityType = next;
    void syncUrlAndSearch(true);
  }

  function retryWithPhonetic(): void {
    strategy = 'phonetic';
    void syncUrlAndSearch(true);
  }

  async function loadMore(): Promise<void> {
    await runSearch(false);
  }

  $: {
    const currentQuery = $page.url.searchParams.toString();
    if (currentQuery !== lastSyncedUrlQuery) {
      parseUrlFilters();
      lastSyncedUrlQuery = currentQuery;
      void runSearch(true);
    }
  }

  onMount(async () => {
    parseUrlFilters();
    lastSyncedUrlQuery = $page.url.searchParams.toString();
    await runSearch(true);
  });
</script>

<main class="panel">
  <header class="header">
    <h1>Search</h1>
    <p>FTS + phonetic search across people, families, events, sources, and notes.</p>
  </header>

  <form class="search-form" on:submit={onSubmit}>
    <input type="search" bind:value={q} placeholder="Search people, places, sources…" />
    <button type="submit">Search</button>
  </form>

  <section class="strategy-row">
    {#each strategyOptions as option}
      <button
        type="button"
        class:active={strategy === option.value}
        on:click={() => setStrategy(option.value)}
      >
        {option.label}
      </button>
    {/each}
  </section>

  <section class="type-tabs">
    {#each entityTabs as tab}
      <button
        type="button"
        class:active={entityType === tab.value}
        on:click={() => setEntityType(tab.value)}
      >
        {tab.label}
      </button>
    {/each}
  </section>

  <section class="filters">
    <label>
      From year
      <input type="number" bind:value={dateFrom} placeholder="e.g. 1800" on:change={() => void syncUrlAndSearch(true)} />
    </label>
    <label>
      To year
      <input type="number" bind:value={dateTo} placeholder="e.g. 1920" on:change={() => void syncUrlAndSearch(true)} />
    </label>
    <label>
      Place
      <input type="text" bind:value={place} placeholder="Boston" on:change={() => void syncUrlAndSearch(true)} />
    </label>
  </section>

  {#if !q.trim()}
    <div class="state empty">
      <p>Enter a search query to begin. Try person surnames, event types, places, or source titles.</p>
    </div>
  {:else}
    {#if error}
      <div class="state error">
        <p>{error}</p>
      </div>
    {:else if loading && results.length === 0}
      <div class="state"><p>Searching…</p></div>
    {:else}
      <p class="summary">Showing {results.length} of {total} results</p>

      {#if results.length === 0}
        <div class="state empty">
          <p>No results for “{q.trim()}”. Try phonetic search.</p>
          <button type="button" on:click={retryWithPhonetic}>Retry with phonetic</button>
        </div>
      {:else}
        <section class="results">
          {#each results as result}
            <a class="result-card" href={detailPathFor(result)}>
              <div class="row top">
                <span class={`entity-badge ${badgeClass(result.entity_type)}`}>{labelForEntity(result.entity_type)}</span>
                {#if strategyUsed === 'phonetic'}
                  <span class="phonetic">phonetic match</span>
                {/if}
              </div>

              <h3>{result.display_name}</h3>

              <div class="fields">
                {#each result.match_fields as field}
                  <span>{field}</span>
                {/each}
              </div>

              {#if result.snippet}
                <p class="snippet">{@html safeSnippetHtml(result.snippet)}</p>
              {/if}

              <div class="score-wrap">
                <div class="score-bar">
                  <span style={`width:${scorePercent(result.score)}%`}></span>
                </div>
                <small>{result.score.toFixed(2)}</small>
              </div>
            </a>
          {/each}
        </section>

        {#if results.length < total}
          <button type="button" class="load-more" disabled={loading} on:click={loadMore}>
            {loading ? 'Loading…' : 'Load more'}
          </button>
        {/if}
      {/if}
    {/if}
  {/if}
</main>

<style>
  .panel {
    background: #ffffff;
    border: 1px solid #e2e8f0;
    border-radius: 0.75rem;
    padding: 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 0.8rem;
  }

  .header h1 {
    margin: 0;
  }

  .header p {
    margin: 0.3rem 0 0;
    color: #64748b;
  }

  .search-form {
    display: flex;
    gap: 0.5rem;
  }

  .search-form input {
    flex: 1;
  }

  .strategy-row,
  .type-tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
  }

  .filters {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.55rem;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    color: #334155;
    font-size: 0.9rem;
  }

  input {
    border: 1px solid #cbd5e1;
    border-radius: 0.42rem;
    padding: 0.45rem 0.55rem;
    font: inherit;
  }

  button {
    border: 0;
    border-radius: 0.45rem;
    background: #2563eb;
    color: #fff;
    padding: 0.42rem 0.68rem;
    cursor: pointer;
    width: fit-content;
  }

  .strategy-row button,
  .type-tabs button {
    background: #f1f5f9;
    color: #0f172a;
    border: 1px solid #cbd5e1;
  }

  .strategy-row button.active,
  .type-tabs button.active {
    background: #dbeafe;
    color: #1e3a8a;
    border-color: #93c5fd;
    font-weight: 600;
  }

  .summary {
    margin: 0;
    color: #334155;
  }

  .state {
    border: 1px dashed #cbd5e1;
    border-radius: 0.6rem;
    padding: 0.7rem;
    background: #f8fafc;
  }

  .state p {
    margin: 0;
  }

  .state.error {
    border-color: #fecaca;
    background: #fff1f2;
    color: #991b1b;
  }

  .results {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 0.65rem;
  }

  .result-card {
    border: 1px solid #e2e8f0;
    border-radius: 0.6rem;
    padding: 0.7rem;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
    cursor: pointer;
    background: #fff;
    text-decoration: none;
    color: inherit;
  }

  .result-card:hover {
    border-color: #bfdbfe;
    box-shadow: 0 6px 20px rgb(15 23 42 / 7%);
  }

  .row.top {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.45rem;
  }

  h3 {
    margin: 0;
    font-size: 1rem;
  }

  .entity-badge,
  .phonetic {
    font-size: 0.72rem;
    font-weight: 700;
    border-radius: 999px;
    padding: 0.14rem 0.45rem;
    text-transform: uppercase;
  }

  .entity-badge.person { background: #dbeafe; color: #1e40af; }
  .entity-badge.family { background: #dcfce7; color: #166534; }
  .entity-badge.event { background: #ffedd5; color: #9a3412; }
  .entity-badge.source { background: #ede9fe; color: #5b21b6; }
  .entity-badge.note { background: #e2e8f0; color: #334155; }
  .entity-badge.default { background: #e2e8f0; color: #334155; }

  .phonetic {
    background: #fef3c7;
    color: #92400e;
  }

  .fields {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }

  .fields span {
    font-size: 0.78rem;
    background: #f1f5f9;
    border-radius: 0.3rem;
    padding: 0.1rem 0.35rem;
    color: #334155;
  }

  .snippet {
    margin: 0;
    color: #475569;
    font-size: 0.88rem;
  }

  .score-wrap {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .score-bar {
    flex: 1;
    height: 0.45rem;
    border-radius: 999px;
    background: #e2e8f0;
    overflow: hidden;
  }

  .score-bar span {
    display: block;
    height: 100%;
    background: linear-gradient(90deg, #ef4444 0%, #f59e0b 55%, #22c55e 100%);
  }

  .score-wrap small {
    color: #475569;
    font-variant-numeric: tabular-nums;
  }

  .load-more {
    align-self: flex-start;
  }
</style>
