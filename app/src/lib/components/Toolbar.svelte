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
    <input bind:value={quickSearch} type="search" placeholder="Quick search…" aria-label="Search records" />
    <button type="submit">Search</button>
  </form>

  <div class="actions">
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
    border-bottom: 1px solid var(--rg-border, #e8def8);
    background: linear-gradient(180deg, #ffffff 0%, #fff8ff 100%);
    padding: 0.82rem 1rem;
  }

  form {
    display: inline-flex;
    gap: 0.5rem;
    flex: 1;
  }

  input {
    width: min(540px, 100%);
    padding: 0.58rem 0.78rem;
    border: 1px solid #dfd2f8;
    border-radius: 999px;
    font: inherit;
    background: #ffffff;
    transition: border-color 140ms ease, box-shadow 140ms ease;
  }

  input:focus {
    outline: none;
    border-color: var(--rg-primary, #9b7bff);
    box-shadow: 0 0 0 3px rgb(155 123 255 / 18%);
  }

  button {
    background: linear-gradient(135deg, var(--rg-primary, #9b7bff), var(--rg-secondary, #ff9fcf));
    color: #ffffff;
    border: 0;
    border-radius: 999px;
    padding: 0.52rem 0.95rem;
    cursor: pointer;
    font: inherit;
    font-weight: 600;
    box-shadow: 0 8px 16px rgb(125 93 242 / 24%);
    transition: transform 120ms ease, filter 120ms ease;
  }

  button:hover {
    transform: translateY(-1px);
    filter: brightness(1.02);
  }

  button:focus-visible,
  input:focus-visible {
    outline: 3px solid rgb(125 93 242 / 45%);
    outline-offset: 2px;
  }

  .actions {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
  }

  .sandbox-badge {
    background: linear-gradient(90deg, #fff2cc 0%, #ffd8e8 100%);
    color: #7a2f62;
    border-radius: 999px;
    border: 1px solid #f8c6dc;
    padding: 0.28rem 0.66rem;
    font-size: 0.8rem;
    font-weight: 650;
  }
</style>