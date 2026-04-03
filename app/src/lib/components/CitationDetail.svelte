<script lang="ts">
  import { onMount } from 'svelte';
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
  };

  export let citationId = '';
  export let citationNote = '';

  let citation: Citation | null = null;
  let source: Source | null = null;
  let loading = false;
  let error = '';

  function confidenceLabel(level: number | null | undefined): string {
    if (level === null || level === undefined) {
      return 'Unknown';
    }
    if (level >= 3) {
      return 'Direct';
    }
    if (level >= 2) {
      return 'Indirect';
    }
    return 'Negative';
  }

  async function load(): Promise<void> {
    if (!citationId) {
      return;
    }
    loading = true;
    error = '';
    try {
      citation = await api.get<Citation>(`/api/v1/citations/${citationId}`);
      source = await api.get<Source>(`/api/v1/sources/${citation.source_id}`);
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load citation detail';
    } finally {
      loading = false;
    }
  }

  onMount(async () => {
    await load();
  });
</script>

<section class="detail">
  <h4>Citation detail</h4>
  {#if loading}
    <p>Loading citation…</p>
  {:else if error}
    <p class="error">{error}</p>
  {:else if citation}
    <p>
      Source:
      {#if source}
        <a href={`/sources/${source.id}`}>{source.title}</a>
      {:else}
        <code>{citation.source_id}</code>
      {/if}
    </p>
    <p>Volume/Page/Folio: {citation.volume ?? '—'} / {citation.page ?? '—'} / {citation.folio ?? '—'}</p>
    <p>Date accessed: {citation.date_accessed ? JSON.stringify(citation.date_accessed) : '—'}</p>
    <p>Quality: {confidenceLabel(citation.confidence_level ?? null)}</p>
    <p>Note: {citationNote || citation.transcription || '—'}</p>
  {/if}
</section>

<style>
  .detail {
    border: 1px solid #cbd5e1;
    border-radius: 0.5rem;
    padding: 0.55rem;
    background: #fff;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  h4,
  p {
    margin: 0;
  }

  a {
    color: #1d4ed8;
    text-decoration: none;
  }

  a:hover {
    text-decoration: underline;
  }

  .error {
    color: #b91c1c;
  }

  code {
    background: #f1f5f9;
    border-radius: 0.25rem;
    padding: 0.1rem 0.3rem;
  }
</style>
