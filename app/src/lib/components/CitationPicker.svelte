<script lang="ts">
  import { createEventDispatcher, onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { CitationDraft } from '$lib/components/formTypes';

  type SourceRef = {
    id: string;
    title: string;
    author?: string | null;
    publication_info?: string | null;
  };

  export let value: CitationDraft[] = [];

  const dispatch = createEventDispatcher<{ change: CitationDraft[] }>();

  let loading = false;
  let error = '';
  let sources: SourceRef[] = [];
  let query = '';
  let selectedSourceId = '';

  let showAddSource = false;
  let newSourceTitle = '';
  let newSourceAuthor = '';
  let newSourcePublication = '';

  let draft: CitationDraft = {
    sourceId: '',
    volume: '',
    page: '',
    folio: '',
    entry: '',
    confidenceLevel: null,
    transcription: '',
    citationNote: ''
  };

  async function loadSources(): Promise<void> {
    loading = true;
    error = '';
    try {
      sources = await api.get<SourceRef[]>('/api/v1/sources?limit=500&offset=0');
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load sources';
    } finally {
      loading = false;
    }
  }

  function filteredSources(): SourceRef[] {
    const trimmed = query.trim().toLowerCase();
    if (!trimmed) {
      return sources.slice(0, 25);
    }

    return sources
      .filter((source) => {
        const haystack = `${source.title} ${source.author ?? ''} ${source.publication_info ?? ''}`.toLowerCase();
        return haystack.includes(trimmed);
      })
      .slice(0, 25);
  }

  async function addSourceInline(): Promise<void> {
    const title = newSourceTitle.trim();
    if (!title) {
      error = 'Source title is required.';
      return;
    }

    error = '';
    try {
      const created = await api.post<{ id: string }>('/api/v1/sources', {
        title,
        author: newSourceAuthor.trim() || null,
        publication_info: newSourcePublication.trim() || null,
        abbreviation: null,
        repository_refs: []
      });
      await loadSources();
      selectedSourceId = created.id;
      draft.sourceId = created.id;
      showAddSource = false;
      newSourceTitle = '';
      newSourceAuthor = '';
      newSourcePublication = '';
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to create source';
    }
  }

  function addCitationDraft(): void {
    const sourceId = (draft.sourceId || selectedSourceId).trim();
    if (!sourceId) {
      error = 'Choose a source before adding a citation.';
      return;
    }

    error = '';
    const next: CitationDraft = {
      sourceId,
      volume: draft.volume.trim(),
      page: draft.page.trim(),
      folio: draft.folio.trim(),
      entry: draft.entry.trim(),
      confidenceLevel: draft.confidenceLevel,
      transcription: draft.transcription.trim(),
      citationNote: draft.citationNote.trim()
    };

    value = [...value, next];
    dispatch('change', value);

    draft = {
      sourceId,
      volume: '',
      page: '',
      folio: '',
      entry: '',
      confidenceLevel: null,
      transcription: '',
      citationNote: ''
    };
  }

  function removeDraft(index: number): void {
    value = value.filter((_, i) => i !== index);
    dispatch('change', value);
  }

  $: if (selectedSourceId) {
    draft.sourceId = selectedSourceId;
  }

  onMount(async () => {
    await loadSources();
  });
</script>

<section class="picker">
  <h3>Citations</h3>

  {#if loading}
    <p>Loading sources…</p>
  {/if}

  {#if error}
    <p class="error">{error}</p>
  {/if}

  <div class="search-row">
    <label>
      Search sources
      <input bind:value={query} placeholder="Title / author" type="search" />
    </label>

    <label>
      Source
      <select bind:value={selectedSourceId}>
        <option value="">Select source</option>
        {#each filteredSources() as source}
          <option value={source.id}>{source.title}</option>
        {/each}
      </select>
    </label>

    <button type="button" class="secondary" on:click={() => (showAddSource = !showAddSource)}>
      {showAddSource ? 'Cancel new source' : 'Add new source'}
    </button>
  </div>

  {#if showAddSource}
    <div class="new-source">
      <input bind:value={newSourceTitle} placeholder="Source title" />
      <input bind:value={newSourceAuthor} placeholder="Author (optional)" />
      <input bind:value={newSourcePublication} placeholder="Publication info (optional)" />
      <button type="button" on:click={addSourceInline}>Create source</button>
    </div>
  {/if}

  <div class="citation-fields">
    <input bind:value={draft.volume} placeholder="Volume" />
    <input bind:value={draft.page} placeholder="Page" />
    <input bind:value={draft.folio} placeholder="Folio" />
    <input bind:value={draft.entry} placeholder="Entry" />
    <input
      type="number"
      min="0"
      max="3"
      placeholder="Confidence (0-3)"
      value={draft.confidenceLevel ?? ''}
      on:input={(event) => {
        const valueRaw = (event.currentTarget as HTMLInputElement).value;
        draft.confidenceLevel = valueRaw === '' ? null : Number(valueRaw);
      }}
    />
    <input bind:value={draft.citationNote} placeholder="Citation note" />
  </div>

  <label>
    Transcription / text
    <textarea rows="2" bind:value={draft.transcription} placeholder="Optional transcription"></textarea>
  </label>

  <button type="button" on:click={addCitationDraft}>Add citation draft</button>

  {#if value.length > 0}
    <ul class="drafts">
      {#each value as citation, i}
        <li>
          <code>{citation.sourceId}</code>
          <span>{citation.page ? `p.${citation.page}` : 'no page'}</span>
          <button type="button" class="danger" on:click={() => removeDraft(i)}>Remove</button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .picker {
    border: 1px solid #cbd5e1;
    border-radius: 0.6rem;
    padding: 0.7rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  h3 {
    margin: 0;
    font-size: 1rem;
  }

  .search-row,
  .citation-fields,
  .new-source {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.45rem;
  }

  .citation-fields {
    grid-template-columns: repeat(6, minmax(0, 1fr));
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.85rem;
  }

  input,
  select,
  textarea {
    border: 1px solid #cbd5e1;
    border-radius: 0.4rem;
    padding: 0.4rem 0.5rem;
    font: inherit;
  }

  button {
    border: 0;
    border-radius: 0.4rem;
    background: #2563eb;
    color: #fff;
    padding: 0.35rem 0.55rem;
    width: fit-content;
    cursor: pointer;
  }

  .secondary {
    background: #475569;
  }

  .danger {
    background: #b91c1c;
  }

  .drafts {
    margin: 0;
    padding-left: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .drafts li {
    display: flex;
    gap: 0.45rem;
    align-items: center;
  }

  .error {
    margin: 0;
    color: #b91c1c;
  }

  code {
    background: #f1f5f9;
    border-radius: 0.3rem;
    padding: 0.05rem 0.25rem;
  }
</style>
