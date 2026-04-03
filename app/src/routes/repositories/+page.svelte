<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { api } from '$lib/api';

  type RepositoryRow = {
    id: string;
    name: string;
    address?: string | null;
    urls: string[];
  };

  let loading = false;
  let saving = false;
  let error = '';
  let repositories: RepositoryRow[] = [];

  let showCreate = false;
  let name = '';
  let address = '';
  let urlText = '';

  async function load(): Promise<void> {
    loading = true;
    error = '';
    try {
      repositories = await api.get<RepositoryRow[]>('/api/v1/repositories?limit=500&offset=0');
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load repositories';
    } finally {
      loading = false;
    }
  }

  async function createRepository(): Promise<void> {
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

      const created = await api.post<{ id: string }>('/api/v1/repositories', {
        name: cleanName,
        repository_type: 'Archive',
        address: address.trim() || null,
        urls
      });

      showCreate = false;
      name = '';
      address = '';
      urlText = '';
      await load();
      await goto(`/repositories/${created.id}`);
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to create repository';
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
    <h1>Repositories</h1>
    <button type="button" on:click={() => (showCreate = !showCreate)}>{showCreate ? 'Close form' : 'New repository'}</button>
  </header>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if showCreate}
    <section class="form">
      <input bind:value={name} placeholder="Name" />
      <textarea rows="2" bind:value={address} placeholder="Address"></textarea>
      <textarea rows="2" bind:value={urlText} placeholder="URLs (one per line)"></textarea>
      <button type="button" on:click={createRepository} disabled={saving}>{saving ? 'Saving…' : 'Save repository'}</button>
    </section>
  {/if}

  {#if loading}
    <p>Loading repositories…</p>
  {:else if repositories.length === 0}
    <p>No repositories yet.</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Name</th>
          <th>Address</th>
          <th>Links</th>
        </tr>
      </thead>
      <tbody>
        {#each repositories as repository}
          <tr>
            <td><a href={`/repositories/${repository.id}`}>{repository.name}</a></td>
            <td>{repository.address ?? '—'}</td>
            <td>{repository.urls.length}</td>
          </tr>
        {/each}
      </tbody>
    </table>
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
    gap: 0.7rem;
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

  table {
    width: 100%;
    border-collapse: collapse;
    border: 1px solid #e2e8f0;
  }

  th,
  td {
    padding: 0.5rem 0.6rem;
    border-bottom: 1px solid #e2e8f0;
    text-align: left;
  }

  a {
    color: #1d4ed8;
    text-decoration: none;
  }

  a:hover {
    text-decoration: underline;
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
