<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { api } from '$lib/api';
  import EventForm from '$lib/components/EventForm.svelte';

  type EventRow = {
    id: string;
    event_type: string;
    date: string | null;
    place_id: string | null;
    participants: { person_id: string; role: string }[];
    confidence: number;
  };

  let rows: EventRow[] = [];
  let filteredRows: EventRow[] = [];
  let loading = false;
  let loadingMore = false;
  let error = '';
  let offset = 0;
  const pageSize = 50;
  let hasMore = true;
  let showCreate = false;

  let typeFilter = '';
  let personFilter = '';
  let fromYear = '';
  let toYear = '';

  function applyFilters(): void {
    filteredRows = rows.filter((event) => {
      if (typeFilter && !event.event_type.toLowerCase().includes(typeFilter.toLowerCase())) {
        return false;
      }

      if (personFilter && !event.participants.some((participant) => participant.person_id === personFilter)) {
        return false;
      }

      const year = event.date?.match(/\d{4}/)?.[0];
      const numericYear = year ? Number(year) : null;

      if (fromYear && numericYear !== null && numericYear < Number(fromYear)) {
        return false;
      }
      if (toYear && numericYear !== null && numericYear > Number(toYear)) {
        return false;
      }

      return true;
    });
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

      const result = await api.get<EventRow[]>(`/api/v1/events?${query.toString()}`);
      rows = reset ? result : [...rows, ...result];
      offset = nextOffset + result.length;
      hasMore = result.length === pageSize;
      applyFilters();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load events';
    } finally {
      loading = false;
      loadingMore = false;
    }
  }

  $: typeFilter, personFilter, fromYear, toYear, applyFilters();

  onMount(async () => {
    await loadPage(true);
  });
</script>

<main class="panel">
  <header class="header">
    <h1>Events</h1>
    <button type="button" on:click={() => (showCreate = true)}>New event</button>
  </header>

  <div class="filters">
    <label>
      Type
      <input bind:value={typeFilter} placeholder="Birth, Census, Marriage…" />
    </label>
    <label>
      Person ID
      <input bind:value={personFilter} placeholder="Participant person UUID" />
    </label>
    <label>
      From year
      <input bind:value={fromYear} placeholder="e.g. 1800" />
    </label>
    <label>
      To year
      <input bind:value={toYear} placeholder="e.g. 1900" />
    </label>
  </div>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if loading}
    <p>Loading events…</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Type</th>
          <th>Date</th>
          <th>Participants</th>
          <th>Confidence</th>
        </tr>
      </thead>
      <tbody>
        {#if filteredRows.length === 0}
          <tr>
            <td colspan="4">No events found.</td>
          </tr>
        {:else}
          {#each filteredRows as event}
            <tr on:click={() => goto(`/events/${event.id}`)}>
              <td>{event.event_type}</td>
              <td>{event.date ?? '—'}</td>
              <td>{event.participants.length}</td>
              <td>{event.confidence.toFixed(2)}</td>
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
  <button type="button" class="overlay" aria-label="Close create event panel" on:click={() => (showCreate = false)}></button>
  <aside class="slideover">
    <EventForm
      mode="create"
      on:cancel={() => (showCreate = false)}
      on:saved={(event: CustomEvent<{ id: string }>) => {
        showCreate = false;
        void goto(`/events/${event.detail.id}`);
      }}
    />
  </aside>
{/if}

<style>
  .panel {
    background: #ffffff;
    border: 1px solid #e2e8f0;
    border-radius: 0.75rem;
    padding: 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .header h1 {
    margin: 0;
  }

  .filters {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 0.5rem;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.85rem;
  }

  input {
    border: 1px solid #cbd5e1;
    border-radius: 0.45rem;
    padding: 0.45rem 0.55rem;
    font: inherit;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    border: 1px solid #e2e8f0;
    border-radius: 0.5rem;
    overflow: hidden;
  }

  th,
  td {
    text-align: left;
    padding: 0.55rem 0.65rem;
    border-bottom: 1px solid #e2e8f0;
  }

  tr {
    cursor: pointer;
  }

  tr:hover {
    background: #f8fafc;
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
    border-left: 1px solid #e2e8f0;
    padding: 1rem;
    overflow: auto;
  }

  .error {
    color: #b91c1c;
    margin: 0;
  }
</style>
