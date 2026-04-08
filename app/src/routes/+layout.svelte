<script lang="ts">
  import Sidebar from '$lib/components/Sidebar.svelte';
  import Toolbar from '$lib/components/Toolbar.svelte';
  import { appState, restoreRecentItems, restoreSandboxState, setCurrentView } from '$lib/state.svelte';
  import { goto } from '$app/navigation';
  import { afterNavigate } from '$app/navigation';
  import { onMount } from 'svelte';
  import { initializeApiClient } from '$lib/api';
  import '../styles/tokens.css';
  import '../styles/global.css';

  let booting = true;
  let startupError = '';

  onMount(async () => {
    try {
      await initializeApiClient();

      restoreRecentItems();
      restoreSandboxState();

      const currentPath = window.location.pathname;
      const lastRoute = localStorage.getItem('last_route');
      if (currentPath === '/' && lastRoute && lastRoute !== '/') {
        await goto(lastRoute, { replaceState: true });
      }
    } catch (err) {
      startupError = err instanceof Error ? err.message : 'Unable to initialize API client';
    } finally {
      booting = false;
    }

    const initialPath = window.location.pathname;
    setCurrentView(initialPath);
    localStorage.setItem('last_route', initialPath);

    afterNavigate(({ to }) => {
      if (!to?.url) {
        return;
      }

      const path = to.url.pathname;
      setCurrentView(path);
      localStorage.setItem('last_route', path);
    });
  });
</script>

{#if booting}
  <main class="centered startup-state">
    <div class="spinner" aria-label="loading"></div>
    <p>Starting RustyGene…</p>
  </main>
{:else if startupError}
  <main class="centered startup-state">
    <h1>API unavailable</h1>
    <p>{startupError}</p>
  </main>
{:else}
  <div class="shell">
    <Sidebar />

    <section class="main-panel">
      <Toolbar />

      {#if appState.pendingRequests > 0}
        <div class="global-loading" aria-live="polite">
          <div class="spinner" aria-label="loading"></div>
          <span>Loading…</span>
        </div>
      {/if}

      <main class="content">
        <slot />
      </main>
    </section>
  </div>
{/if}

<style>
  .centered {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.75rem;
  }

  .startup-state {
    background: transparent;
  }

  .shell {
    min-height: 100vh;
    display: grid;
    grid-template-columns: 280px 1fr;
    gap: 1rem;
    padding: 1rem;
    box-sizing: border-box;
  }

  .main-panel {
    display: flex;
    flex-direction: column;
    min-width: 0;
    border: 1px solid var(--rg-border);
    background: var(--rg-surface);
    border-radius: var(--radius-xl);
    box-shadow: var(--rg-shadow);
    overflow: hidden;
  }

  .global-loading {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    margin: 0.85rem 1rem 0;
    padding: 0.5rem 0.85rem;
    border-radius: var(--radius-pill);
    background: rgb(155 123 255 / 14%);
    color: var(--rg-primary-strong);
    border: 1px solid rgb(155 123 255 / 28%);
    width: fit-content;
  }

  .content {
    padding: 1.2rem;
    background: linear-gradient(180deg, #ffffff 0%, #fffaff 100%);
  }

  .spinner {
    width: 2rem;
    height: 2rem;
    border: 3px solid #d6dcf0;
    border-top-color: var(--rg-primary);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
