<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { addRecentItem } from '$lib/state.svelte';
  import { api } from '$lib/api';
  import NoteList from '$lib/components/NoteList.svelte';

  type Citation = {
    id: string;
    source_id: string;
    volume?: string | null;
    page?: string | null;
    folio?: string | null;
    entry?: string | null;
  };

  type SourceDetail = {
    id: string;
    title: string;
    author?: string | null;
    publication_info?: string | null;
    abbreviation?: string | null;
    repository_refs: Array<{ repository_id: string; call_number?: string | null; media_type?: string | null }>;
    citations: Citation[];
  };

  type Repository = {
    id: string;
    name: string;
  };

  let id = '';
  $: id = $page.params.id ?? '';

  let loading = false;
  let saving = false;
  let deleting = false;
  let error = '';
  let detail: SourceDetail | null = null;
  let repositories: Repository[] = [];
  let editing = false;

  let title = '';
  let author = '';
  let publicationInfo = '';
  let abbreviation = '';
  let repositoryId = '';
  let callNumber = '';

  function repositoryName(repoId: string): string {
    return repositories.find((repo) => repo.id === repoId)?.name ?? repoId;
  }

  function seedFormFromDetail(): void {
    if (!detail) {
      return;
    }

    title = detail.title;
    author = detail.author ?? '';
    publicationInfo = detail.publication_info ?? '';
    abbreviation = detail.abbreviation ?? '';
    repositoryId = detail.repository_refs[0]?.repository_id ?? '';
    callNumber = detail.repository_refs[0]?.call_number ?? '';
  }

  async function loadDetail(): Promise<void> {
    loading = true;
    error = '';
    try {
      const [sourceDetail, repositoryRows] = await Promise.all([
        api.get<SourceDetail>(`/api/v1/sources/${id}`),
        api.get<Repository[]>(`/api/v1/repositories?limit=500&offset=0`)
      ]);
      detail = sourceDetail;
      repositories = repositoryRows;
      seedFormFromDetail();

      addRecentItem({
        entityType: 'source',
        id,
        displayName: sourceDetail.title
      });
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load source detail';
    } finally {
      loading = false;
    }
  }

  async function save(): Promise<void> {
    if (!detail) {
      return;
    }

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

      await api.put(`/api/v1/sources/${id}`, {
        title: cleanTitle,
        author: author.trim() || null,
        publication_info: publicationInfo.trim() || null,
        abbreviation: abbreviation.trim() || null,
        repository_refs
      });

      editing = false;
      await loadDetail();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to save source';
    } finally {
      saving = false;
    }
  }

  async function removeSource(): Promise<void> {
    const confirmed = confirm('Delete this source?');
    if (!confirmed) {
      return;
    }

    deleting = true;
    error = '';
    try {
      await api.del(`/api/v1/sources/${id}`);
      await goto('/sources');
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to delete source';
    } finally {
      deleting = false;
    }
  }

  onMount(() => {
    void loadDetail();
  });
</script>

{#if loading}
  <p>Loading source detail…</p>
{:else if error}
  <main class="panel">
    <h1>Source detail</h1>
    <p class="error">{error}</p>
  </main>
{:else if detail}
  <main class="panel">
    <header class="header">
      <div>
        <h1>{detail.title}</h1>
        <p>ID: <code>{detail.id}</code></p>
      </div>
      <div class="actions">
        <button type="button" class="secondary" on:click={() => (editing = !editing)}>{editing ? 'Cancel' : 'Edit'}</button>
        <button type="button" class="danger" disabled={deleting} on:click={removeSource}>{deleting ? 'Deleting…' : 'Delete'}</button>
      </div>
    </header>

    {#if editing}
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
        <input bind:value={callNumber} placeholder="Call number" />
        <button type="button" on:click={save} disabled={saving}>{saving ? 'Saving…' : 'Save changes'}</button>
      </section>
    {/if}

    <section>
      <h2>Repository links</h2>
      {#if detail.repository_refs.length === 0}
        <p>No repositories linked.</p>
      {:else}
        <ul class="list">
          {#each detail.repository_refs as repoRef}
            <li>
              <a href={`/repositories/${repoRef.repository_id}`}>{repositoryName(repoRef.repository_id)}</a>
              {#if repoRef.call_number}
                <span class="muted">(call no. {repoRef.call_number})</span>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <h2>Citations</h2>
      {#if detail.citations.length === 0}
        <p>No citations found for this source.</p>
      {:else}
        <ul class="list">
          {#each detail.citations as citation}
            <li>
              <code>{citation.id}</code>
              <span>
                · entity/field back-link available on assertion detail
                {citation.page ? `· p.${citation.page}` : ''}
                {citation.volume ? `· vol.${citation.volume}` : ''}
              </span>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <NoteList entityId={id} entityType="source" />
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
    gap: 0.9rem;
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 0.8rem;
  }

  .header h1,
  .header p {
    margin: 0;
  }

  .actions {
    display: flex;
    gap: 0.45rem;
  }

  section {
    border: 1px solid #efe6ff;
    border-radius: 0.85rem;
    padding: 0.85rem;
    background: #fffdff;
  }

  section h2 {
    margin-top: 0;
    color: #593ca8;
  }

  .form {
    border: 1px solid #dfd2f8;
    border-radius: 0.8rem;
    background: #fffdff;
    padding: 0.72rem;
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.5rem;
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
    border-radius: 0.7rem;
    background: #2563eb;
    color: #fff;
    padding: 0.4rem 0.65rem;
    cursor: pointer;
    width: fit-content;
  }

  .secondary {
    background: #7258c7;
  }

  .danger {
    background: #d03165;
  }

  .list {
    margin: 0;
    padding-left: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .list a {
    color: #6a46dc;
    font-weight: 600;
    text-decoration: none;
  }

  .list a:hover {
    text-decoration: underline;
  }

  .muted {
    color: #6b6192;
    margin-left: 0.35rem;
  }

  .error {
    color: #b91c1c;
    margin: 0;
  }

  code {
    background: #f5efff;
    padding: 0.1rem 0.3rem;
    border-radius: 0.35rem;
  }
</style>
