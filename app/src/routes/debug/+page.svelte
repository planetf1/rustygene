<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { api } from '$lib/api';
  import { appState } from '$lib/state.svelte';

  type DependencyStatus = {
    ok: boolean;
    detail: string;
  };

  type MigrationStatus = {
    ok: boolean;
    present_tables: number;
    missing_tables: string[];
  };

  type ConfigSnapshot = {
    db_path: string;
    media_dir: string;
    cors_origins: string[];
    sandbox_mode_hint: string;
  };

  type DebugHealthDepsResponse = {
    debug_enabled: boolean;
    app_version: string;
    git_commit: string | null;
    api_port: number;
    db: DependencyStatus;
    migrations: MigrationStatus;
    media_dir: DependencyStatus;
    config_snapshot: ConfigSnapshot;
  };

  type RouteMetric = {
    route: string;
    request_count: number;
    average_latency_ms: number;
  };

  type DebugMetricsResponse = {
    total_requests: number;
    routes: RouteMetric[];
    import_jobs: {
      queued: number;
      running: number;
      completed: number;
      failed: number;
    };
  };

  type DebugLogEntry = {
    timestamp: string;
    level: string;
    target: string;
    message: string;
    fields: unknown;
  };

  const LEVELS = ['ALL', 'ERROR', 'WARN', 'INFO', 'DEBUG', 'TRACE'] as const;

  let loading = false;
  let error = '';
  let message = '';

  let health: DebugHealthDepsResponse | null = null;
  let metrics: DebugMetricsResponse | null = null;
  let logs: DebugLogEntry[] = [];
  let levelFilter: (typeof LEVELS)[number] = 'INFO';
  let pollHandle: number | null = null;

  async function loadHealth(): Promise<void> {
    health = await api.get<DebugHealthDepsResponse>('/api/v1/debug/health/deps');
  }

  async function loadMetrics(): Promise<void> {
    metrics = await api.get<DebugMetricsResponse>('/api/v1/debug/metrics');
  }

  async function loadLogs(): Promise<void> {
    const suffix = levelFilter === 'ALL' ? '' : `?level=${encodeURIComponent(levelFilter)}`;
    logs = await api.get<DebugLogEntry[]>(`/api/v1/debug/logs${suffix}`);
  }

  async function refreshAll(): Promise<void> {
    loading = true;
    error = '';
    message = '';

    try {
      await Promise.all([loadHealth(), loadMetrics(), loadLogs()]);
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load debug diagnostics';
    } finally {
      loading = false;
    }
  }

  async function downloadDiagnostics(): Promise<void> {
    error = '';
    message = '';
    try {
      const file = await api.download('/api/v1/debug/bundle');
      const href = URL.createObjectURL(file.blob);
      const anchor = document.createElement('a');
      anchor.href = href;
      anchor.download = file.fileName ?? 'rustygene-diagnostics.json';
      anchor.click();
      URL.revokeObjectURL(href);
      message = 'Diagnostics bundle downloaded.';
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to download diagnostics bundle';
    }
  }

  async function copyDiagnosticsBundle(): Promise<void> {
    error = '';
    message = '';
    try {
      const file = await api.download('/api/v1/debug/bundle');
      const text = await file.blob.text();
      await navigator.clipboard.writeText(text);
      message = 'Diagnostics bundle copied to clipboard.';
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to copy diagnostics bundle';
    }
  }

  function clearLocalUiCache(): void {
    localStorage.removeItem('recent_items');
    localStorage.removeItem('last_route');
    message = 'Local UI cache cleared.';
  }

  function statusText(status: DependencyStatus | null): string {
    if (!status) {
      return 'unknown';
    }
    return status.ok ? 'OK' : 'FAILED';
  }

  onMount(async () => {
    await refreshAll();

    pollHandle = window.setInterval(() => {
      void Promise.all([loadLogs(), loadMetrics()]);
    }, 2000);
  });

  onDestroy(() => {
    if (pollHandle !== null) {
      window.clearInterval(pollHandle);
    }
  });
</script>

<main class="panel">
  <header>
    <h1>Debug Console</h1>
    <p>Diagnostics, metrics, and live logs for local troubleshooting.</p>
  </header>

  <section class="actions">
    <button type="button" on:click={() => void refreshAll()} disabled={loading}>Trigger health check</button>
    <button type="button" class="secondary" on:click={() => void copyDiagnosticsBundle()}>Copy diagnostics bundle</button>
    <button type="button" class="secondary" on:click={() => void downloadDiagnostics()}>Download diagnostics bundle</button>
    <button type="button" class="danger" on:click={clearLocalUiCache}>Clear local UI cache</button>
  </section>

  {#if message}
    <p class="ok">{message}</p>
  {/if}
  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if loading && !health}
    <p>Loading debug diagnostics…</p>
  {:else if health}
    <section class="cards">
      <article>
        <h2>Runtime</h2>
        <dl>
          <dt>API port</dt>
          <dd>{appState.apiPort ?? health.api_port}</dd>
          <dt>App version</dt>
          <dd>{health.app_version}</dd>
          <dt>Git commit</dt>
          <dd>{health.git_commit ?? 'n/a'}</dd>
          <dt>Sandbox</dt>
          <dd>{appState.sandboxMode ? 'sandbox=1 (enabled)' : 'default (disabled)'}</dd>
          <dt>Database path</dt>
          <dd>{health.config_snapshot.db_path}</dd>
        </dl>
      </article>

      <article>
        <h2>Dependencies</h2>
        <dl>
          <dt>DB connectivity</dt>
          <dd>{statusText(health.db)} — {health.db.detail}</dd>
          <dt>Migrations</dt>
          <dd>{health.migrations.ok ? 'OK' : 'FAILED'} ({health.migrations.present_tables} tables)</dd>
          <dt>Media directory</dt>
          <dd>{statusText(health.media_dir)} — {health.media_dir.detail}</dd>
        </dl>
        {#if health.migrations.missing_tables.length > 0}
          <p class="warn">Missing tables: {health.migrations.missing_tables.join(', ')}</p>
        {/if}
      </article>

      <article>
        <h2>Import jobs</h2>
        {#if metrics}
          <dl>
            <dt>Queued</dt>
            <dd>{metrics.import_jobs.queued}</dd>
            <dt>Running</dt>
            <dd>{metrics.import_jobs.running}</dd>
            <dt>Completed</dt>
            <dd>{metrics.import_jobs.completed}</dd>
            <dt>Failed</dt>
            <dd>{metrics.import_jobs.failed}</dd>
          </dl>
        {:else}
          <p>Metrics unavailable.</p>
        {/if}
      </article>
    </section>

    <section class="metrics">
      <h2>Route metrics</h2>
      {#if metrics && metrics.routes.length > 0}
        <table>
          <thead>
            <tr>
              <th>Route</th>
              <th>Requests</th>
              <th>Avg latency (ms)</th>
            </tr>
          </thead>
          <tbody>
            {#each metrics.routes as route}
              <tr>
                <td>{route.route}</td>
                <td>{route.request_count}</td>
                <td>{route.average_latency_ms}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <p>No route metrics collected yet.</p>
      {/if}
    </section>

    <section class="logs">
      <div class="log-head">
        <h2>Live logs</h2>
        <label>
          Level
          <select bind:value={levelFilter} on:change={() => void loadLogs()}>
            {#each LEVELS as level}
              <option value={level}>{level}</option>
            {/each}
          </select>
        </label>
      </div>

      <div class="log-list">
        {#if logs.length === 0}
          <p>No logs available for current filter.</p>
        {:else}
          {#each logs as entry}
            <article class="log-entry">
              <div class="row">
                <span class="level">{entry.level}</span>
                <span class="ts">{entry.timestamp}</span>
                <span class="target">{entry.target}</span>
              </div>
              <p>{entry.message}</p>
            </article>
          {/each}
        {/if}
      </div>
    </section>
  {/if}
</main>

<style>
  .panel {
    background: #ffffff;
    border: 1px solid #e2e8f0;
    border-radius: 0.75rem;
    padding: 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
  }

  header h1 {
    margin: 0;
  }

  header p {
    margin: 0.25rem 0 0;
    color: #64748b;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  button {
    border: 1px solid #cbd5e1;
    background: #eff6ff;
    color: #1e3a8a;
    border-radius: 0.5rem;
    padding: 0.45rem 0.7rem;
  }

  .secondary {
    background: #f8fafc;
    color: #0f172a;
  }

  .danger {
    background: #fee2e2;
    color: #991b1b;
  }

  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
    gap: 0.75rem;
  }

  article {
    border: 1px solid #e2e8f0;
    border-radius: 0.5rem;
    padding: 0.75rem;
  }

  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.25rem 0.5rem;
    margin: 0;
  }

  dt {
    color: #475569;
    font-weight: 600;
  }

  dd {
    margin: 0;
    word-break: break-word;
  }

  .metrics table {
    width: 100%;
    border-collapse: collapse;
  }

  .metrics th,
  .metrics td {
    border-bottom: 1px solid #e2e8f0;
    padding: 0.4rem;
    text-align: left;
  }

  .log-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .log-list {
    max-height: 380px;
    overflow: auto;
    border: 1px solid #e2e8f0;
    border-radius: 0.5rem;
    padding: 0.5rem;
    background: #0f172a;
    color: #e2e8f0;
  }

  .log-entry {
    border: 1px solid #1e293b;
    border-radius: 0.4rem;
    padding: 0.4rem 0.5rem;
    margin-bottom: 0.4rem;
    background: #111827;
  }

  .log-entry p {
    margin: 0.2rem 0 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.85rem;
  }

  .row {
    display: flex;
    gap: 0.5rem;
    font-size: 0.75rem;
    color: #93c5fd;
  }

  .ok {
    color: #166534;
  }

  .error {
    color: #991b1b;
  }

  .warn {
    color: #9a3412;
    margin-top: 0.5rem;
  }
</style>
