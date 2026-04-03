<script lang="ts">
  import { goto } from '$app/navigation';
  import { appState } from '$lib/state.svelte';

  let quickSearch = '';

  async function runSearch(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const value = quickSearch.trim();
    if (!value) {
      await goto('/search');
      return;
    }

    await goto(`/search?q=${encodeURIComponent(value)}`);
  }
</script>

<header class="toolbar">
  <form on:submit={runSearch}>
    <input bind:value={quickSearch} type="search" placeholder="Quick search…" />
    <button type="submit">Search</button>
  </form>

  <div class="actions">
    <a href="/import">Import</a>
    <a href="/export">Export</a>
    {#if appState.sandboxMode}
      <span class="sandbox-badge">Sandbox</span>
    {/if}
  </div>
</header>

<style>
  .toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.75rem;
    border-bottom: 1px solid #e2e8f0;
    background: #ffffff;
    padding: 0.75rem 1rem;
  }

  form {
    display: inline-flex;
    gap: 0.5rem;
    flex: 1;
  }

  input {
    width: min(540px, 100%);
    padding: 0.5rem 0.65rem;
    border: 1px solid #cbd5e1;
    border-radius: 0.45rem;
    font: inherit;
  }

  button,
  .actions a {
    background: #2563eb;
    color: #ffffff;
    border: 0;
    border-radius: 0.45rem;
    padding: 0.5rem 0.75rem;
    text-decoration: none;
    cursor: pointer;
    font: inherit;
  }

  .actions {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
  }

  .sandbox-badge {
    background: #fef3c7;
    color: #92400e;
    border-radius: 999px;
    padding: 0.25rem 0.6rem;
    font-size: 0.8rem;
    font-weight: 600;
  }
</style>