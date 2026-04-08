<script lang="ts">
  import { onMount } from 'svelte';
  import { appState, setSandboxMode } from '$lib/state.svelte';
  import { api } from '$lib/api';
  import { page } from '$app/state';

  let recentOpen = true;
  let openResearchEntryCount = 0;

  const navItems = [
    { href: '/persons', label: 'People' },
    { href: '/families', label: 'Families' },
    { href: '/events', label: 'Events' },
    { href: '/entry/source-driven', label: 'Source-driven Entry' },
    { href: '/sources', label: 'Sources' },
    { href: '/repositories', label: 'Repositories' },
    { href: '/staging', label: 'Review Queue' },
    { href: '/research-log', label: 'Research Log' },
    { href: '/sandboxes', label: 'Sandboxes' },
    { href: '/backup', label: 'Backup' },
    { href: '/search', label: 'Search' },
    { href: '/debug', label: 'Debug' }
  ];

  async function refreshResearchOpenCount(): Promise<void> {
    try {
      const result = await api.get<{ total: number } | Array<{ id: string }>>('/api/v1/research-log?status=open&limit=1&offset=0');
      // Handle both paginated ({total}) and array response shapes
      if (result && !Array.isArray(result) && typeof (result as {total: number}).total === 'number') {
        openResearchEntryCount = (result as {total: number}).total;
      } else if (Array.isArray(result)) {
        // Fallback: if API returns plain array, fetch with higher limit but cap display
        const all = await api.get<Array<{ id: string }>>('/api/v1/research-log?status=open&limit=100&offset=0');
        openResearchEntryCount = all.length;
      } else {
        openResearchEntryCount = 0;
      }
    } catch {
      openResearchEntryCount = 0;
    }
  }

  onMount(() => {
    void refreshResearchOpenCount();
  });

  const chartItems = [
    { href: '/charts/pedigree', label: 'Pedigree' },
    { href: '/charts/fan', label: 'Fan' },
    { href: '/charts/graph', label: 'Relationship Graph' }
  ];

  const transferItems = [
    { href: '/import', label: 'Import' },
    { href: '/export', label: 'Export' }
  ];

  function iconFor(type: string): string {
    switch (type) {
      case 'person':
        return '👤';
      case 'family':
        return '👪';
      case 'event':
        return '📅';
      case 'source':
        return '📚';
      case 'repository':
        return '🏛️';
      default:
        return '•';
    }
  }

  function routeForRecent(type: string, id: string): string {
    switch (type) {
      case 'person':
        return `/persons/${id}`;
      case 'family':
        return `/families/${id}`;
      case 'event':
        return `/events/${id}`;
      case 'source':
        return `/sources/${id}`;
      case 'repository':
        return `/repositories/${id}`;
      default:
        return '/';
    }
  }
</script>

<aside class="sidebar">
  <header>
    <h1>RustyGene</h1>
    <p>Desktop shell</p>
  </header>

  <nav class="section" aria-label="Main navigation">
    {#each navItems as item}
      <a class:selected={page.url.pathname.startsWith(item.href)} href={item.href}>
        <span>{item.label}</span>
        {#if item.href === '/research-log' && openResearchEntryCount > 0}
          <span class="badge" aria-label={`${openResearchEntryCount} open research log entries`}>{openResearchEntryCount}</span>
        {/if}
      </a>
    {/each}
  </nav>

  <div class="section">
    <h2>Charts</h2>
    {#each chartItems as item}
      <a class:selected={page.url.pathname.startsWith(item.href)} href={item.href}>{item.label}</a>
    {/each}
  </div>

  <div class="section" aria-label="Data transfer">
    <h2>Data Transfer</h2>
    {#each transferItems as item}
      <a class:selected={page.url.pathname.startsWith(item.href)} href={item.href}>{item.label}</a>
    {/each}
  </div>

  <footer class="footer">
    <label class="sandbox-toggle">
      <input
        type="checkbox"
        checked={appState.sandboxMode}
        on:change={(event) => setSandboxMode((event.currentTarget as HTMLInputElement).checked)}
      />
      <span>Sandbox mode</span>
    </label>

    <button class="recent-toggle" type="button" on:click={() => (recentOpen = !recentOpen)}>
      Recent items {recentOpen ? '▾' : '▸'}
    </button>

    {#if recentOpen}
      <ul class="recent-list">
        {#if appState.recentItems.length === 0}
          <li class="empty">No recently visited items.</li>
        {:else}
          {#each appState.recentItems.slice(0, 5) as item}
            <li>
              <a href={routeForRecent(item.entityType, item.id)}>
                <span>{iconFor(item.entityType)}</span>
                <span>{item.displayName}</span>
              </a>
            </li>
          {/each}
        {/if}
      </ul>
    {/if}
  </footer>
</aside>

<style>
  .sidebar {
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    border-radius: 1.25rem;
    box-shadow: var(--shadow-md);
    padding: 1.1rem;
    display: flex;
    flex-direction: column;
    gap: 1.1rem;
  }

  header h1 {
    margin: 0;
    font-size: 1.26rem;
    letter-spacing: -0.01em;
    color: var(--color-text);
    font-family: var(--font-family-heading);
    font-weight: 600;
  }

  header p {
    margin: 0.25rem 0 0;
    color: var(--color-muted);
    font-size: 0.88rem;
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: 0.38rem;
  }

  .section h2 {
    margin: 0.45rem 0 0.25rem;
    font-size: 0.75rem;
    color: var(--color-muted);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  a {
    color: var(--color-text);
    text-decoration: none;
    padding: 0.52rem 0.68rem;
    border-radius: 0.72rem;
    border: 1px solid transparent;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
    transition: all 140ms ease;
    font-weight: 500;
  }

  a:hover {
    background: var(--color-surface-soft);
    border-color: var(--color-border);
    transform: translateX(1px);
  }

  a:focus-visible {
    background: var(--color-surface-soft);
    border-color: var(--color-primary);
    box-shadow: 0 0 0 3px rgb(79 70 229 / 20%);
    outline: none;
  }

  a.selected {
    background: var(--color-surface-soft);
    border-color: var(--color-border);
    color: var(--color-primary-strong);
    font-weight: 600;
  }

  .footer {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
  }

  .sandbox-toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    color: var(--color-text);
    font-size: 0.93rem;
  }

  .recent-toggle {
    background: var(--color-surface-soft);
    border: 1px solid var(--color-border);
    border-radius: 0.68rem;
    padding: 0.5rem 0.68rem;
    text-align: left;
    color: var(--color-text);
    font-weight: 550;
    transition: filter 140ms ease;
    cursor: pointer;
  }

  .recent-toggle:hover {
    filter: brightness(0.96);
  }

  .recent-toggle:focus-visible,
  .sandbox-toggle input:focus-visible {
    outline: 2px solid var(--color-primary);
    outline-offset: 2px;
  }

  .recent-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .recent-list a {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.91rem;
  }

  .empty {
    color: var(--rg-muted, #64748b);
    font-size: 0.85rem;
    padding: 0.25rem 0;
  }

  .badge {
    background: #fef2f2;
    border: 1px solid #fecaca;
    color: #b91c1c;
    border-radius: 999px;
    font-size: 0.72rem;
    line-height: 1;
    padding: 0.18rem 0.4rem;
    min-width: 1.2rem;
    text-align: center;
    font-weight: 700;
  }
</style>