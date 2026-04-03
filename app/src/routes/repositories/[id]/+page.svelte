<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { addRecentItem } from '$lib/state.svelte';
  import { api } from '$lib/api';
  import NoteList from '$lib/components/NoteList.svelte';

  type Repository = {
    id: string;
    name: string;
    address?: string | null;
    urls: string[];
  };

  type Source = {
    id: string;
    title: string;
    repository_refs: Array<{ repository_id: string; call_number?: string | null; media_type?: string | null }>;
  };

  $: id = $page.params.id;

  let detail: Repository | null = null;
  let allSources: Source[] = [];
  let loading = false;
  let saving = false;
  let deleting = false;
  let editing = false;
  let error = '';

  let name = '';
  let address = '';
  let urlText = '';

  function linkedSources(): Source[] {
    if (!detail) {
      return [];
    }

    return allSources.filter((source) => source.repository_refs.some((ref) => ref.repository_id === detail?.id));
  }

  function seedForm(): void {
    if (!detail) {
      return;
    }

    name = detail.name;
    address = detail.address ?? '';
    urlText = detail.urls.join('\n');
  }

  async function load(): Promise<void> {
    loading = true;
    error = '';
    try {
      const [repository, sources] = await Promise.all([
        api.get<Repository>(`/api/v1/repositories/${id}`),
        api.get<Source[]>('/api/v1/sources?limit=1000&offset=0')
      ]);
      detail = repository;
      allSources = sources;
      seedForm();

      addRecentItem({
        entityType: 'repository',
        id,
        displayName: repository.name
      });
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load repository detail';
    } finally {
      loading = false;
    }
  }

  async function save(): Promise<void> {
    const cleanName = name.trim();
    if (!cleanName) {
      error = 'Repository name is required.';
      return;
    }

    saving = true;
    error = '';
    try {
      const urls = urlText
        .split('\n')
        .map((value) => value.trim())
        .filter((value) => value.length > 0);

      await api.put(`/api/v1/repositories/${id}`, {
        name: cleanName,
        repository_type: 'Archive',
        address: address.trim() || null,
        urls
      });
      editing = false;
      await load();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to save repository';
    } finally {
      saving = false;
    }
  }

  async function removeRepository(): Promise<void> {
    const confirmed = confirm('Delete this repository?');
    if (!confirmed) {
      return;
    }

    deleting = true;
    error = '';
    try {
      await api.del(`/api/v1/repositories/${id}`);
      await goto('/repositories');
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to delete repository';
    } finally {
      deleting = false;
    }
  }

  onMount(async () => {
    await load();
  });
</script>

{#if loading}
  <p>Loading repository detail…</p>
{:else if error}
  <main class="panel">
    <h1>Repository detail</h1>
    <p class="error">{error}</p>
  </main>
{:else if detail}
  <main class="panel">
    <header class="header">
      <div>
        <h1>{detail.name}</h1>
        <p>ID: <code>{detail.id}</code></p>
      </div>
      <div class="actions">
        <button type="button" class="secondary" on:click={() => (editing = !editing)}>{editing ? 'Cancel' : 'Edit'}</button>
        <button type="button" class="danger" disabled={deleting} on:click={removeRepository}>{deleting ? 'Deleting…' : 'Delete'}</button>
      </div>
    </header>

    {#if editing}
      <section class="form">
        <input bind:value={name} placeholder="Name" />
        <textarea rows="2" bind:value={address} placeholder="Address"></textarea>
        <textarea rows="3" bind:value={urlText} placeholder="URLs (one per line)"></textarea>
        <button type="button" disabled={saving} on:click={save}>{saving ? 'Saving…' : 'Save changes'}</button>
      </section>
    {/if}

    <section>
      <h2>Address & contact</h2>
      <p>{detail.address ?? 'No address available.'}</p>
      {#if detail.urls.length > 0}
        <ul class="list">
          {#each detail.urls as url}
            <li><a href={url} target="_blank" rel="noreferrer">{url}</a></li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <h2>Sources held here</h2>
      {#if linkedSources().length === 0}
        <p>No linked sources.</p>
      {:else}
        <ul class="list">
          {#each linkedSources() as source}
            <li><a href={`/sources/${source.id}`}>{source.title}</a></li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <NoteList entityId={id} entityType="repository" />
    </section>
  </main>
{/if}

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

  .form {
    border: 1px solid #cbd5e1;
    border-radius: 0.6rem;
    padding: 0.65rem;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.45rem;
  }

  input,
  textarea {
    border: 1px solid #cbd5e1;
    border-radius: 0.45rem;
    padding: 0.45rem 0.55rem;
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

  .secondary {
    background: #475569;
  }

  .danger {
    background: #b91c1c;
  }

  .list {
    margin: 0;
    padding-left: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
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
    margin: 0;
  }

  code {
    background: #f1f5f9;
    border-radius: 0.25rem;
    padding: 0.1rem 0.3rem;
  }
</style>
