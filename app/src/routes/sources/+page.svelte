<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { api } from '$lib/api';

  type SourceRow = {
    id: string;
    title: string;
    author?: string | null;
    publication_info?: string | null;
    abbreviation?: string | null;
    repository_refs: Array<{ repository_id: string; call_number?: string | null; media_type?: string | null }>;
  };

  type CitationRow = {
    id: string;
    source_id: string;
  };

  type RepositoryRow = {
    id: string;
    name: string;
  };

  let loading = false;
  let saving = false;
  let error = '';
  let sources: SourceRow[] = [];
  let citations: CitationRow[] = [];
  let repositories: RepositoryRow[] = [];

  let showCreate = false;
  let title = '';
  let author = '';
  let publicationInfo = '';
  let abbreviation = '';
  let repositoryId = '';
  let callNumber = '';

  function citationCountFor(sourceId: string): number {
    return citations.filter((citation) => citation.source_id === sourceId).length;
  }

  function repositoryName(id: string): string {
    return repositories.find((row) => row.id === id)?.name ?? id;
  }

  async function load(): Promise<void> {
    loading = true;
    error = '';
    try {
      const [sourceRows, citationRows, repositoryRows] = await Promise.all([
        api.get<SourceRow[]>('/api/v1/sources?limit=500&offset=0'),
        api.get<CitationRow[]>('/api/v1/citations?limit=2000&offset=0'),
        api.get<RepositoryRow[]>('/api/v1/repositories?limit=500&offset=0')
      ]);
      sources = sourceRows;
      citations = citationRows;
      repositories = repositoryRows;
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load sources';
    } finally {
      loading = false;
    }
  }

  async function createSource(): Promise<void> {
    const cleanTitle = title.trim();
    if (!cleanTitle) {
      error = 'Source title is required.';
      return;
    }

    saving = true;
    error = '';

    try {
      const repository_refs = repositoryId
        ? [
            {
              repository_id: repositoryId,
              call_number: callNumber.trim() || null,
              media_type: null
            }
          ]
        : [];

      const created = await api.post<{ id: string }>('/api/v1/sources', {
        title: cleanTitle,
        author: author.trim() || null,
        publication_info: publicationInfo.trim() || null,
        abbreviation: abbreviation.trim() || null,
        repository_refs
      });

      showCreate = false;
      title = '';
      author = '';
      publicationInfo = '';
      abbreviation = '';
      repositoryId = '';
      callNumber = '';
      await load();
      await goto(`/sources/${created.id}`);
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to create source';
    } finally {
      saving = false;
    }
  }

  onMount(async () => {
    await load();
  });
</script>

<main class="panel">
  <header>
    <h1>Sources</h1>
    <button type="button" on:click={() => (showCreate = !showCreate)}>
      {showCreate ? 'Close form' : 'New source'}
    </button>
  </header>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if showCreate}
    <section class="form">
      <input bind:value={title} placeholder="Title" />
      <input bind:value={author} placeholder="Author" />
      <input bind:value={publicationInfo} placeholder="Publication info" />
      <input bind:value={abbreviation} placeholder="Abbreviation" />
      <select bind:value={repositoryId}>
        <option value="">No repository</option>
        {#each repositories as repository}
          <option value={repository.id}>{repository.name}</option>
        {/each}
      </select>
      <input bind:value={callNumber} placeholder="Call number (optional)" />
      <button type="button" on:click={createSource} disabled={saving}>{saving ? 'Saving…' : 'Save source'}</button>
    </section>
  {/if}

  {#if loading}
    <p>Loading sources…</p>
  {:else if sources.length === 0}
    <p>No sources yet.</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Title</th>
          <th>Repository</th>
          <th>Citations</th>
        </tr>
      </thead>
      <tbody>
        {#each sources as source}
          <tr>
            <td><a href={`/sources/${source.id}`}>{source.title}</a></td>
            <td>
              {#if source.repository_refs.length === 0}
                —
              {:else}
                {repositoryName(source.repository_refs[0].repository_id)}
              {/if}
            </td>
            <td>{citationCountFor(source.id)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</main>

<style>
  .panel {
    background: linear-gradient(180deg, #ffffff 0%, #fff9ff 100%);
    border: 1px solid var(--rg-border, #e8def8);
    border-radius: 1rem;
    padding: 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }

  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  h1 {
    margin: 0;
  }

  .form {
    border: 1px solid #dfd2f8;
    border-radius: 0.85rem;
    background: #fffdff;
    padding: 0.72rem;
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.52rem;
  }

  table {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
    border: 1px solid var(--rg-border, #e8def8);
    border-radius: 0.85rem;
    overflow: hidden;
  }

  thead th {
    background: linear-gradient(180deg, #f9f2ff 0%, #fff1f9 100%);
    color: #55389a;
  }

  th,
  td {
    padding: 0.5rem 0.6rem;
    border-bottom: 1px solid #f0e8ff;
    text-align: left;
  }

  a {
    color: #6a46dc;
    text-decoration: none;
    font-weight: 600;
  }

  a:hover {
    text-decoration: underline;
  }

  input,
  select {
    border: 1px solid #dfd2f8;
    border-radius: 0.7rem;
    padding: 0.5rem 0.62rem;
    font: inherit;
  }

  button {
    border: 0;
    border-radius: 0.45rem;
    background: #2563eb;
    color: #fff;
    padding: 0.4rem 0.65rem;
    cursor: pointer;
    width: fit-content;
  }

  .error {
    margin: 0;
    color: #b91c1c;
  }
</style>
