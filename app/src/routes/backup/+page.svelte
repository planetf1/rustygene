<script lang="ts">
  import { onMount } from 'svelte';
  import { api, ApiError } from '$lib/api';

  type BackupInfo = {
    filename: string;
    size_bytes: number;
    created_at: number;
  };

  let backups: BackupInfo[] = [];
  let loading = false;
  let creating = false;
  let message = '';
  let error = '';

  let restoreFile: FileList | null = null;
  let restoring = false;
  let confirmRestore: BackupInfo | null = null;
  let confirmDelete: BackupInfo | null = null;

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  function formatDate(unixSecs: number): string {
    return new Date(unixSecs * 1000).toLocaleString();
  }

  async function fetchBackups(): Promise<void> {
    loading = true;
    error = '';
    try {
      backups = await api.get<BackupInfo[]>('/api/v1/backup');
    } catch (err) {
      error = err instanceof ApiError ? err.message : 'Failed to list backups';
    } finally {
      loading = false;
    }
  }

  async function createBackup(): Promise<void> {
    creating = true;
    message = '';
    error = '';
    try {
      const info = await api.post<BackupInfo>('/api/v1/backup', null);
      message = `Backup created: ${info.filename} (${formatBytes(info.size_bytes)})`;
      await fetchBackups();
    } catch (err) {
      error = err instanceof ApiError ? err.message : 'Backup failed';
    } finally {
      creating = false;
    }
  }

  async function doRestore(): Promise<void> {
    if (!confirmRestore && !restoreFile) return;

    restoring = true;
    message = '';
    error = '';

    try {
      if (confirmRestore) {
        // Restore from a server-side backup
        const blob = await fetch(api.url(`/api/v1/backup/${encodeURIComponent(confirmRestore.filename)}`)).then(
          (r) => r.blob()
        );
        const fd = new FormData();
        fd.append('file', blob, confirmRestore.filename);
        await api.postFormData<void>('/api/v1/backup/restore', fd);
        message = `Restored from ${confirmRestore.filename}. Please reload the app to see the restored data.`;
      } else if (restoreFile?.[0]) {
        const fd = new FormData();
        fd.append('file', restoreFile[0]);
        await api.postFormData<void>('/api/v1/backup/restore', fd);
        message = `Restored from ${restoreFile[0].name}. Please reload the app to see the restored data.`;
      }
    } catch (err) {
      error = err instanceof ApiError ? err.message : 'Restore failed';
    } finally {
      restoring = false;
      confirmRestore = null;
      restoreFile = null;
    }
  }

  async function doDelete(backup: BackupInfo): Promise<void> {
    error = '';
    message = '';
    try {
      await api.del<void>(`/api/v1/backup/${encodeURIComponent(backup.filename)}`);
      message = `Deleted backup: ${backup.filename}`;
      await fetchBackups();
    } catch (err) {
      error = err instanceof ApiError ? err.message : 'Delete failed';
    } finally {
      confirmDelete = null;
    }
  }

  onMount(fetchBackups);
</script>

<svelte:head>
  <title>Backup &amp; Restore</title>
</svelte:head>

<h1>Backup &amp; Restore</h1>
<p class="subtitle">Create point-in-time database backups and restore from a previous snapshot.</p>

{#if message}
  <p class="success">{message}</p>
{/if}
{#if error}
  <p class="error">{error}</p>
{/if}

<section>
  <h2>Create Backup</h2>
  <p>Creates a consistent SQLite snapshot of the current database in the server-side backups directory.</p>
  <button type="button" on:click={createBackup} disabled={creating}>
    {creating ? 'Creating…' : 'Create Backup Now'}
  </button>
</section>

<section>
  <h2>Restore from File</h2>
  <p>Upload a previously downloaded <code>.db</code> backup file to overwrite the current database.</p>
  <input type="file" accept=".db" bind:files={restoreFile} />
  {#if restoreFile?.[0]}
    <button
      type="button"
      class="danger"
      on:click={() => {
        confirmRestore = null;
        doRestore();
      }}
      disabled={restoring}
    >
      {restoring ? 'Restoring…' : `Restore from ${restoreFile[0].name}`}
    </button>
  {/if}
</section>

<section>
  <h2>Available Backups</h2>

  {#if loading}
    <p>Loading…</p>
  {:else if backups.length === 0}
    <p>No backups found.</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Filename</th>
          <th>Size</th>
          <th>Created</th>
          <th>Actions</th>
        </tr>
      </thead>
      <tbody>
        {#each backups as backup (backup.filename)}
          <tr>
            <td><code>{backup.filename}</code></td>
            <td>{formatBytes(backup.size_bytes)}</td>
            <td>{formatDate(backup.created_at)}</td>
            <td class="actions">
              <button
                type="button"
                on:click={() => {
                  confirmRestore = backup;
                }}
                disabled={restoring}
                title="Restore this backup"
              >
                Restore
              </button>
              <button
                type="button"
                class="danger"
                on:click={() => {
                  confirmDelete = backup;
                }}
                title="Delete this backup"
              >
                Delete
              </button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</section>

<!-- Restore confirmation dialog -->
{#if confirmRestore}
  <div class="overlay" role="dialog" aria-modal="true">
    <div class="dialog">
      <h3>Confirm Restore</h3>
      <p>
        Restore from <strong>{confirmRestore.filename}</strong>? This will overwrite all current
        data. This action cannot be undone.
      </p>
      <div class="dialog-actions">
        <button type="button" class="danger" on:click={doRestore} disabled={restoring}>
          {restoring ? 'Restoring…' : 'Yes, restore'}
        </button>
        <button
          type="button"
          on:click={() => {
            confirmRestore = null;
          }}
          disabled={restoring}
        >
          Cancel
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Delete confirmation dialog -->
{#if confirmDelete}
  <div class="overlay" role="dialog" aria-modal="true">
    <div class="dialog">
      <h3>Delete Backup</h3>
      <p>
        Permanently delete <strong>{confirmDelete.filename}</strong>? This cannot be undone.
      </p>
      <div class="dialog-actions">
        <button
          type="button"
          class="danger"
          on:click={() => confirmDelete && doDelete(confirmDelete)}
        >
          Yes, delete
        </button>
        <button
          type="button"
          on:click={() => {
            confirmDelete = null;
          }}
        >
          Cancel
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  h1 {
    margin-bottom: 0.25rem;
  }

  .subtitle {
    color: #6b7280;
    margin-bottom: 1.5rem;
  }

  section {
    background: #fff;
    border: 1px solid #e5e7eb;
    border-radius: 0.5rem;
    padding: 1.25rem 1.5rem;
    margin-bottom: 1.5rem;
  }

  section h2 {
    margin: 0 0 0.5rem;
    font-size: 1.1rem;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9rem;
  }

  th,
  td {
    padding: 0.5rem 0.75rem;
    text-align: left;
    border-bottom: 1px solid #f3f4f6;
  }

  th {
    font-weight: 600;
    background: #f9fafb;
  }

  td.actions {
    display: flex;
    gap: 0.5rem;
  }

  button {
    padding: 0.4rem 0.9rem;
    border: 1px solid #d1d5db;
    border-radius: 0.375rem;
    background: #fff;
    cursor: pointer;
    font-size: 0.875rem;
  }

  button:disabled {
    opacity: 0.6;
    cursor: default;
  }

  button.danger {
    color: #dc2626;
    border-color: #fca5a5;
  }

  button.danger:hover:not(:disabled) {
    background: #fee2e2;
  }

  .success {
    color: #15803d;
    background: #dcfce7;
    padding: 0.5rem 1rem;
    border-radius: 0.375rem;
    margin-bottom: 1rem;
  }

  .error {
    color: #b91c1c;
    background: #fee2e2;
    padding: 0.5rem 1rem;
    border-radius: 0.375rem;
    margin-bottom: 1rem;
  }

  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 50;
  }

  .dialog {
    background: #fff;
    border-radius: 0.5rem;
    padding: 1.5rem;
    max-width: 28rem;
    width: 90%;
    box-shadow: 0 10px 25px rgba(0, 0, 0, 0.15);
  }

  .dialog h3 {
    margin: 0 0 0.75rem;
  }

  .dialog-actions {
    display: flex;
    gap: 0.75rem;
    margin-top: 1.25rem;
  }
</style>
