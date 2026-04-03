<script lang="ts">
  import Sidebar from '$lib/components/Sidebar.svelte';
  import Toolbar from '$lib/components/Toolbar.svelte';
  import { appState, restoreRecentItems, restoreSandboxState, setCurrentView } from '$lib/state.svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { afterNavigate } from '$app/navigation';
  import { onMount } from 'svelte';
  import { initializeApiClient } from '$lib/api';

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
  :global(:root) {
    --rg-bg: #fff7fb;
    --rg-surface: #ffffff;
    --rg-surface-soft: #fff9ff;
    --rg-text: #2a2342;
    --rg-muted: #756f95;
    --rg-border: #e8def8;
    --rg-primary: #9b7bff;
    --rg-primary-strong: #7d5df2;
    --rg-secondary: #ff9fcf;
    --rg-tertiary: #8ee7d1;
    --rg-success: #10b981;
    --rg-warning: #f59e0b;
    --rg-shadow: 0 16px 34px rgb(125 93 242 / 12%);
  }

  :global(body) {
    margin: 0;
    font-family: Inter, 'Avenir Next', 'Nunito', system-ui, -apple-system, sans-serif;
    color: var(--rg-text);
    background:
      radial-gradient(circle at 6% 10%, rgb(155 123 255 / 20%) 0%, transparent 28%),
      radial-gradient(circle at 90% 12%, rgb(255 159 207 / 20%) 0%, transparent 30%),
      radial-gradient(circle at 80% 82%, rgb(142 231 209 / 22%) 0%, transparent 32%),
      linear-gradient(180deg, #fff9ff 0%, var(--rg-bg) 100%);
  }

  :global(a:focus-visible),
  :global(button:focus-visible),
  :global(input:focus-visible),
  :global(select:focus-visible),
  :global(textarea:focus-visible) {
    outline: 3px solid rgb(125 93 242 / 45%);
    outline-offset: 2px;
  }

  :global(.content h1),
  :global(.content h2),
  :global(.content h3) {
    color: #4f3a9f;
    letter-spacing: 0.01em;
  }

  :global(.content p) {
    color: var(--rg-text);
  }

  :global(.content .panel),
  :global(.content .card),
  :global(.content .surface) {
    border: 1px solid var(--rg-border);
    border-radius: 1rem;
    box-shadow: 0 10px 24px rgb(125 93 242 / 10%);
    background: linear-gradient(180deg, #ffffff 0%, #fffaff 100%);
  }

  :global(.content label) {
    color: #594f7d;
    font-weight: 600;
  }

  :global(.content input),
  :global(.content select),
  :global(.content textarea) {
    border: 1px solid #dfd2f8;
    border-radius: 0.7rem;
    padding: 0.5rem 0.62rem;
    background: #fff;
    color: var(--rg-text);
    transition: border-color 120ms ease, box-shadow 120ms ease;
  }

  :global(.content input:focus),
  :global(.content select:focus),
  :global(.content textarea:focus) {
    outline: none;
    border-color: var(--rg-primary);
    box-shadow: 0 0 0 3px rgb(155 123 255 / 17%);
  }

  :global(.content table) {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
    border: 1px solid var(--rg-border);
    border-radius: 0.9rem;
    overflow: hidden;
    background: #fff;
  }

  :global(.content thead th) {
    background: linear-gradient(180deg, #f9f2ff 0%, #fff0f8 100%);
    color: #55389a;
    border-bottom: 1px solid var(--rg-border);
  }

  :global(.content th),
  :global(.content td) {
    text-align: left;
    padding: 0.6rem 0.7rem;
    border-bottom: 1px solid #f0e8ff;
  }

  :global(.content tbody tr:hover) {
    background: #fdf7ff;
  }

  :global(.content button:not(.overlay)) {
    border: 0;
    border-radius: 0.7rem;
    background: linear-gradient(135deg, var(--rg-primary), var(--rg-secondary));
    color: #fff;
    font-weight: 600;
    box-shadow: 0 8px 16px rgb(125 93 242 / 22%);
    transition: transform 120ms ease, filter 120ms ease;
  }

  :global(.content button:not(.overlay):hover) {
    transform: translateY(-1px);
    filter: brightness(1.02);
  }

  @media (prefers-reduced-motion: reduce) {
    :global(*),
    :global(*::before),
    :global(*::after) {
      animation-duration: 0.01ms !important;
      animation-iteration-count: 1 !important;
      transition-duration: 0.01ms !important;
      scroll-behavior: auto !important;
    }
  }

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
    border-radius: 1.2rem;
    box-shadow: var(--rg-shadow);
    overflow: hidden;
  }

  .global-loading {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    margin: 0.85rem 1rem 0;
    padding: 0.5rem 0.85rem;
    border-radius: 999px;
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
