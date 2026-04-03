<script lang="ts">
  import { api } from '$lib/api';

  type ExportFormat = 'gedcom' | 'json' | 'bundle';

  type TauriInvoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

  let format: ExportFormat = 'gedcom';
  let redactLiving = false;
  let outputFileName = defaultFileName('gedcom');
  let previousFormat: ExportFormat = format;

  let busyExport = false;
  let busyBackup = false;
  let busyRestore = false;

  let message = '';
  let error = '';

  $: if (format !== previousFormat) {
    outputFileName = defaultFileName(format);
    previousFormat = format;
  }

  function inTauri(): boolean {
    return typeof window !== 'undefined' && Boolean(window.__TAURI_INTERNALS__);
  }

  async function invokeTauri<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    const { invoke } = await import('@tauri-apps/api/core');
    const tauriInvoke = invoke as TauriInvoke;
    return tauriInvoke<T>(command, args);
  }

  function extensionFor(nextFormat: ExportFormat): string {
    switch (nextFormat) {
      case 'gedcom':
        return 'ged';
      case 'json':
        return 'json';
      case 'bundle':
        return 'zip';
    }
  }

  function todayIso(): string {
    return new Date().toISOString().slice(0, 10);
  }

  function defaultFileName(nextFormat: ExportFormat): string {
    return `rustygene-export-${todayIso()}.${extensionFor(nextFormat)}`;
  }

  function defaultBackupFileName(): string {
    return `rustygene-backup-${todayIso()}.db`;
  }

  async function saveBlobBrowser(blob: Blob, fileName: string): Promise<void> {
    const url = URL.createObjectURL(blob);
    try {
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = fileName;
      anchor.style.display = 'none';
      document.body.append(anchor);
      anchor.click();
      anchor.remove();
    } finally {
      URL.revokeObjectURL(url);
    }
  }

  async function saveBlobTauri(blob: Blob, suggestedName: string): Promise<string | null> {
    const targetPath = await invokeTauri<string | null>('save_file_dialog', {
      title: 'Save export file',
      defaultName: suggestedName
    });

    if (!targetPath) {
      return null;
    }

    const arrayBuffer = await blob.arrayBuffer();
    const bytes = Array.from(new Uint8Array(arrayBuffer));
    await invokeTauri<void>('write_binary_file', {
      path: targetPath,
      bytes
    });

    return targetPath;
  }

  async function runExport(): Promise<void> {
    busyExport = true;
    error = '';
    message = '';

    try {
      const { blob, fileName } = await api.download(
        `/api/v1/export?format=${encodeURIComponent(format)}&redact_living=${redactLiving}`
      );

      const resolvedName = outputFileName.trim() || fileName || defaultFileName(format);

      if (inTauri()) {
        const savedPath = await saveBlobTauri(blob, resolvedName);
        if (!savedPath) {
          message = 'Export canceled.';
          return;
        }

        message = `Saved to ${savedPath}`;
      } else {
        await saveBlobBrowser(blob, resolvedName);
        message = `Download started: ${resolvedName}`;
      }
    } catch (err) {
      error = err instanceof Error ? err.message : 'Export failed';
    } finally {
      busyExport = false;
    }
  }

  async function createBackup(): Promise<void> {
    busyBackup = true;
    error = '';
    message = '';

    try {
      if (!inTauri()) {
        throw new Error('Backup is only available in the desktop app.');
      }

      const destinationPath = await invokeTauri<string | null>('save_file_dialog', {
        title: 'Create database backup',
        defaultName: defaultBackupFileName()
      });

      if (!destinationPath) {
        message = 'Backup canceled.';
        return;
      }

      await invokeTauri<void>('create_database_backup', {
        destinationPath
      });

      message = `Backup saved to ${destinationPath}`;
    } catch (err) {
      error = err instanceof Error ? err.message : 'Backup failed';
    } finally {
      busyBackup = false;
    }
  }

  async function restoreBackup(): Promise<void> {
    busyRestore = true;
    error = '';
    message = '';

    try {
      if (!inTauri()) {
        throw new Error('Restore is only available in the desktop app.');
      }

      const confirmed = window.confirm(
        'This will REPLACE your current data. Are you sure you want to continue?'
      );
      if (!confirmed) {
        message = 'Restore canceled.';
        return;
      }

      const sourcePath = await invokeTauri<string | null>('open_file_dialog', {
        title: 'Select backup database',
        filters: ['db']
      });

      if (!sourcePath) {
        message = 'Restore canceled.';
        return;
      }

      await invokeTauri<void>('restore_database_backup', {
        sourcePath
      });

      message = 'Backup restored. Reloading app…';
      window.location.reload();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Restore failed';
    } finally {
      busyRestore = false;
    }
  }
</script>

<main class="panel">
  <h1>Export &amp; backup</h1>
  <p class="subtitle">Export GEDCOM/JSON/bundle, create DB backups, and restore from backup.</p>

  <section class="section">
    <h2>Export data</h2>

    <fieldset>
      <legend>Format</legend>
      <label><input type="radio" bind:group={format} value="gedcom" /> GEDCOM 5.5.1 (.ged)</label>
      <label><input type="radio" bind:group={format} value="json" /> JSON (.json)</label>
      <label><input type="radio" bind:group={format} value="bundle" /> Media Bundle (.zip)</label>
    </fieldset>

    <label class="checkbox">
      <input type="checkbox" bind:checked={redactLiving} />
      <span>Redact living persons</span>
    </label>

    <label>
      Output filename
      <input type="text" bind:value={outputFileName} />
    </label>

    <div class="actions">
      <button type="button" on:click={runExport} disabled={busyExport}>
        {busyExport ? 'Exporting…' : 'Export'}
      </button>
    </div>
  </section>

  <section class="section">
    <h2>Backup</h2>
    <p>Create a point-in-time SQLite database backup file.</p>
    <button type="button" on:click={createBackup} disabled={busyBackup}>
      {busyBackup ? 'Creating backup…' : 'Create Backup'}
    </button>
  </section>

  <section class="section">
    <h2>Restore</h2>
    <p>Restoring will replace the active database and restart the embedded API.</p>
    <button type="button" class="danger" on:click={restoreBackup} disabled={busyRestore}>
      {busyRestore ? 'Restoring…' : 'Restore from Backup'}
    </button>
  </section>

  {#if message}
    <p class="ok">{message}</p>
  {/if}
  {#if error}
    <p class="error">{error}</p>
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
    gap: 1rem;
  }

  h1,
  h2 {
    margin: 0;
  }

  .subtitle {
    margin: 0;
    color: #475569;
  }

  .section {
    border: 1px solid #e2e8f0;
    border-radius: 0.65rem;
    padding: 0.85rem;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  fieldset {
    border: 1px solid #cbd5e1;
    border-radius: 0.55rem;
    padding: 0.6rem;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
  }

  legend {
    color: #334155;
    font-size: 0.88rem;
    padding: 0 0.25rem;
  }

  label {
    display: inline-flex;
    flex-direction: column;
    gap: 0.35rem;
    color: #334155;
  }

  .checkbox {
    display: inline-flex;
    flex-direction: row;
    align-items: center;
    gap: 0.5rem;
  }

  input[type='text'] {
    border: 1px solid #cbd5e1;
    border-radius: 0.4rem;
    padding: 0.45rem 0.55rem;
    font: inherit;
  }

  button {
    border: 0;
    border-radius: 0.45rem;
    padding: 0.45rem 0.75rem;
    background: #2563eb;
    color: #fff;
    cursor: pointer;
    width: fit-content;
  }

  button.danger {
    background: #b91c1c;
  }

  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .ok {
    margin: 0;
    color: #166534;
  }

  .error {
    margin: 0;
    color: #b91c1c;
  }
</style>
