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

  let families: FamilyRow[] = [];
  let loading = false;
  let loadingMore = false;
  let error = '';
  let offset = 0;
  const pageSize = 50;
  let hasMore = true;
  let showCreate = false;

  function marriageYear(row: FamilyRow): string {
    const marriage = row.events.find((event) => event.event_type.toLowerCase().includes('marriage'));
    if (!marriage?.date) {
      return '—';
    }

    return marriage.date.match(/\d{4}/)?.[0] ?? '—';
  }

  function familyLabel(row: FamilyRow): string {
    const p1 = row.partner1?.display_name ?? 'Unknown';
    const p2 = row.partner2?.display_name ?? 'Unknown';
    return `${p1} + ${p2}`;
  }

  async function loadPage(reset = false): Promise<void> {
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

      const rows = await api.get<FamilyRow[]>(`/api/v1/families?${query.toString()}`);
      families = reset ? rows : [...families, ...rows];
      offset = nextOffset + rows.length;
      hasMore = rows.length === pageSize;
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load families';
    } finally {
      loading = false;
      loadingMore = false;
    }
  }

  onMount(async () => {
    await loadPage(true);
  });
</script>

<main class="panel">
  <header class="header">
    <h1>Families</h1>
    <button type="button" on:click={() => (showCreate = true)}>New family</button>
  </header>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if loading}
    <p>Loading families…</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Family</th>
          <th>Marriage year</th>
          <th>Child count</th>
        </tr>
      </thead>
      <tbody>
        {#if families.length === 0}
          <tr>
            <td colspan="3">No families found.</td>
          </tr>
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

    {#if hasMore}
      <button type="button" class="load-more" disabled={loadingMore} on:click={() => loadPage(false)}>
        {loadingMore ? 'Loading…' : 'Load more'}
      </button>
    {/if}
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
