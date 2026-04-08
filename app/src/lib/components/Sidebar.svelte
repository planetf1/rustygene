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
      const rows = await api.get<Array<{ id: string }>>('/api/v1/research-log?status=open&limit=500&offset=0');
      openResearchEntryCount = rows.length;
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
    border: 1px solid var(--rg-border, #dbe3f1);
    background:
      linear-gradient(180deg, #ffffff 0%, #fff6ff 100%);
    border-radius: 1.25rem;
    box-shadow: 0 16px 30px rgb(125 93 242 / 13%);
    padding: 1.1rem;
    display: flex;
    flex-direction: column;
    gap: 1.1rem;
  }

  header h1 {
    margin: 0;
    font-size: 1.26rem;
    letter-spacing: 0.01em;
    color: #4529a8;
  }

  header p {
    margin: 0.25rem 0 0;
    color: var(--rg-muted, #64748b);
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
    color: var(--rg-muted, #5e557e);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  a {
    color: var(--rg-text, #172036);
    text-decoration: none;
    padding: 0.52rem 0.68rem;
    border-radius: 0.72rem;
    border: 1px solid transparent;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
    transition: background 140ms ease, border-color 140ms ease, transform 140ms ease, box-shadow 140ms ease;
  }

  a:hover {
    background: #f9f2ff;
    border-color: #e7d6ff;
    transform: translateX(2px);
    box-shadow: 0 4px 10px rgb(155 123 255 / 12%);
  }

  a:focus-visible {
    background: #f9f2ff;
    border-color: #c9b1ff;
    box-shadow: 0 0 0 3px rgb(125 93 242 / 24%);
  }

  a.selected {
    background: linear-gradient(90deg, rgb(155 123 255 / 18%), rgb(255 159 207 / 16%));
    border-color: rgb(155 123 255 / 30%);
    color: #3f249d;
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
    color: var(--rg-text, #172036);
    font-size: 0.93rem;
  }

  .recent-toggle {
    background: #f7efff;
    border: 1px solid #e7d6ff;
    border-radius: 0.68rem;
    padding: 0.5rem 0.68rem;
    text-align: left;
    color: #402772;
    font-weight: 550;
  }

  .recent-toggle:hover {
    filter: brightness(0.98);
  }

  .recent-toggle:focus-visible,
  .sandbox-toggle input:focus-visible {
    outline: 3px solid rgb(125 93 242 / 45%);
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