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

  let people: PersonRow[] = [];
  let filteredPeople: PersonRow[] = [];
  let error = '';
  let loading = false;
  let loadingMore = false;
  let searchText = '';
  let offset = 0;
  const pageSize = 50;
  let hasMore = true;
  let showCreatePanel = false;

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  async function fetchPage(reset = false): Promise<void> {
    const nextOffset = reset ? 0 : offset;

    if (reset) {
      loading = true;
      error = '';
    } else {
      loadingMore = true;
    }

    try {
      const query = new URLSearchParams({
        limit: String(pageSize),
        offset: String(nextOffset)
      });

      if (searchText.trim()) {
        query.set('q', searchText.trim());
      }

      const rows = await api.get<PersonRow[]>(`/api/v1/persons?${query.toString()}`);
      const nextRows = reset ? rows : [...people, ...rows];

      people = nextRows;
      offset = nextOffset + rows.length;
      hasMore = rows.length === pageSize;

      applyClientFilter();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load persons';
    } finally {
      loading = false;
      loadingMore = false;
    }
  }

  function applyClientFilter(): void {
    const query = searchText.trim().toLowerCase();
    if (!query) {
      filteredPeople = [...people];
      return;
    }

    filteredPeople = people.filter((person) => person.display_name.toLowerCase().includes(query));
  }

  function onSearchInput(): void {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }

    debounceTimer = setTimeout(() => {
      void fetchPage(true);
    }, 300);
  }

  function assertionCount(row: PersonRow): number {
    return Object.values(row.assertion_counts).reduce((sum, value) => sum + value, 0);
  }

  onMount(async () => {
    await fetchPage(true);
  });
</script>

<main class="panel">
  <header class="header">
    <h1>Persons</h1>
    <button type="button" on:click={() => (showCreatePanel = true)}>New Person</button>
  </header>

  <label class="search-box">
    <span>Inline filter</span>
    <input
      bind:value={searchText}
      on:input={onSearchInput}
      placeholder="Search display name…"
      type="search"
    />
  </label>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if loading}
    <p>Loading persons…</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Display name</th>
          <th>Birth year</th>
          <th>Death year</th>
          <th>Assertion count</th>
        </tr>
      </thead>
      <tbody>
        {#if filteredPeople.length === 0}
          <tr>
            <td colspan="4">No persons found.</td>
          </tr>
        {:else}
          {#each filteredPeople as person}
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

    {#if hasMore}
      <button type="button" class="load-more" on:click={() => fetchPage(false)} disabled={loadingMore}>
        {loadingMore ? 'Loading…' : 'Load more'}
      </button>
    {/if}
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
    gap: 0.95rem;
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .header h1 {
    margin: 0;
  }

  .search-box {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    font-size: 0.9rem;
    color: #5a4f7d;
    font-weight: 600;
  }

  input {
    border: 1px solid #dfd2f8;
    border-radius: 0.7rem;
    padding: 0.5rem 0.62rem;
    font: inherit;
    max-width: 26rem;
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
    text-align: left;
    padding: 0.55rem 0.65rem;
    border-bottom: 1px solid #f0e8ff;
  }

  tr {
    cursor: pointer;
  }

  tr:hover {
    background: #fdf7ff;
  }

  button {
    background: #2563eb;
    color: #fff;
    border: 0;
    border-radius: 0.45rem;
    padding: 0.45rem 0.7rem;
    cursor: pointer;
    width: fit-content;
  }

  .load-more {
    align-self: flex-start;
  }

  .overlay {
    position: fixed;
    inset: 0;
    background: rgb(15 23 42 / 35%);
    border: 0;
    width: 100%;
    padding: 0;
    border-radius: 0;
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
