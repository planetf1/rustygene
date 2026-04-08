<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { api } from '$lib/api';

  type Citation = {
    id: string;
    source_id: string;
    volume?: string | null;
    page?: string | null;
    folio?: string | null;
    entry?: string | null;
    confidence_level?: number | null;
    date_accessed?: unknown;
    transcription?: string | null;
  };

  type Source = {
    id: string;
    title: string;
    author?: string | null;
    publication_info?: string | null;
  };

  let id = '';
  $: id = $page.params.id ?? '';

  let citation: Citation | null = null;
  let source: Source | null = null;
  let loading = false;
  let error = '';
  let backLabel = '';
  let backHref = '';

  function confidenceLabel(level: number | null | undefined): string {
    if (level === null || level === undefined) return 'Unknown';
    if (level >= 3) return 'Direct';
    if (level >= 2) return 'Indirect';
    return 'Negative';
  }

  function readBackContext(): void {
    const from = $page.url.searchParams.get('from') ?? '';
    const back = $page.url.searchParams.get('back') ?? '';
    backLabel = from ? `← Back to ${from}` : '← Back';
    backHref = back;
  }

  async function load(): Promise<void> {
    loading = true;
    error = '';

    try {
      const row = await api.get<Citation>(`/api/v1/citations/${id}`);
      citation = row;

      try {
        source = await api.get<Source>(`/api/v1/sources/${row.source_id}`);
      } catch {
        source = null;
      }
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load citation detail';
    } finally {
      loading = false;
    }
  }

  onMount(async () => {
    readBackContext();
    await load();
  });
</script>

{#if loading}
  <p class="loading-msg">Loading citation…</p>
{:else if error}
  <main class="panel">
    <h1>Citation detail</h1>
    <p class="error">{error}</p>
  </main>
{:else if citation}
  <main class="panel">
    {#if backHref}
      <button type="button" class="back-link" on:click={() => goto(backHref)}>{backLabel}</button>
    {/if}

    <header class="header">
      <div>
        <h1>Citation {citation.id}</h1>
        <p>Confidence: <span class="badge">{confidenceLabel(citation.confidence_level ?? null)}</span></p>
      </div>
      <button type="button" class="btn-source" on:click={() => goto(`/sources/${citation?.source_id ?? ''}`)}>
        Open source
      </button>
    </header>

    <section class="section-card">
      <h2 class="section-title">Source</h2>
      {#if source}
        <p><strong>{source.title}</strong></p>
        <p class="muted">{source.author ?? 'Unknown author'}</p>
        <p class="muted">{source.publication_info ?? 'No publication info'}</p>
      {:else}
        <p class="muted">Source {citation.source_id}</p>
      {/if}
    </section>

    <section class="section-card">
      <h2 class="section-title">Locator</h2>
      <dl class="fact-grid">
        <dt>Volume</dt><dd>{citation.volume ?? '—'}</dd>
        <dt>Page</dt><dd>{citation.page ?? '—'}</dd>
        <dt>Folio</dt><dd>{citation.folio ?? '—'}</dd>
        <dt>Entry</dt><dd>{citation.entry ?? '—'}</dd>
      </dl>
    </section>

    <section class="section-card">
      <h2 class="section-title">Transcription / note</h2>
      <p>{citation.transcription ?? '—'}</p>
    </section>
  </main>
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

  .loading-msg { color: #888; margin: 0; }

  .header {
    display: flex;
    justify-content: space-between;
    gap: 0.8rem;
    align-items: flex-start;
  }

  .header h1 { margin: 0; font-size: 1.25rem; }
  .header p { margin: 0.25rem 0 0; color: #6b5fa0; }

  .badge {
    background: #eef2ff;
    border: 1px solid #c7d2fe;
    border-radius: 999px;
    padding: 0.12rem 0.4rem;
    font-size: 0.8rem;
    color: #3730a3;
  }

  .btn-source {
    border: 1px solid #dfd2f8;
    background: #f3edff;
    color: #5b21b6;
    border-radius: 0.45rem;
    padding: 0.35rem 0.65rem;
    cursor: pointer;
    font-size: 0.85rem;
  }

  .back-link {
    align-self: flex-start;
    background: transparent;
    border: 0;
    color: #4c1d95;
    padding: 0;
    cursor: pointer;
    font: inherit;
    text-decoration: underline;
    font-size: 0.85rem;
  }

  .section-card {
    border: 1px solid #efe6ff;
    border-radius: 0.75rem;
    padding: 0.85rem 1rem;
    background: #fffdff;
  }

  .section-title {
    margin: 0 0 0.55rem;
    font-size: 0.9rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: #593ca8;
  }

  .fact-grid {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.3rem 1rem;
    margin: 0;
    font-size: 0.9rem;
  }

  dt { font-weight: 600; color: #5a4f7d; }
  dd { margin: 0; color: #1e1037; }

  .muted { color: #6b5fa0; margin: 0.1rem 0; }

  .error { color: #b91c1c; margin: 0; }
</style>
