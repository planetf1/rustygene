<script lang="ts">
  import { appState, setSandboxMode } from '$lib/state.svelte';
  import { page } from '$app/stores';

  let recentOpen = true;

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

  const chartItems = [
    { href: '/charts/pedigree', label: 'Pedigree' },
    { href: '/charts/fan', label: 'Fan' },
    { href: '/charts/graph', label: 'Relationship Graph' }
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
      <a class:selected={$page.url.pathname.startsWith(item.href)} href={item.href}>{item.label}</a>
    {/each}
  </nav>

  <div class="section">
    <h2>Charts</h2>
    {#each chartItems as item}
      <a class:selected={$page.url.pathname.startsWith(item.href)} href={item.href}>{item.label}</a>
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
    border-right: 1px solid #e2e8f0;
    background: #ffffff;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  header h1 {
    margin: 0;
    font-size: 1.15rem;
  }

  header p {
    margin: 0.25rem 0 0;
    color: #64748b;
    font-size: 0.875rem;
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .section h2 {
    margin: 0.5rem 0 0.25rem;
    font-size: 0.85rem;
    color: #64748b;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  a {
    color: #0f172a;
    text-decoration: none;
    padding: 0.4rem 0.5rem;
    border-radius: 0.4rem;
  }

  a:hover {
    background: #f1f5f9;
  }

  a.selected {
    background: #dbeafe;
    color: #1e3a8a;
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
  }

  .recent-toggle {
    background: #f1f5f9;
    border: 1px solid #e2e8f0;
    border-radius: 0.4rem;
    padding: 0.35rem 0.5rem;
    text-align: left;
    color: #334155;
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
    font-size: 0.9rem;
  }

  .empty {
    color: #64748b;
    font-size: 0.85rem;
    padding: 0.25rem 0;
  }
</style>