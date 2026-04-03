<script lang="ts">
  import { onMount } from 'svelte';
  import { api, ApiError } from '$lib/api';
  import { appState, setSandboxMode } from '$lib/state.svelte';

  type SandboxStatus = 'active' | 'promoted' | 'discarded';

  type Sandbox = {
    id: string;
    name: string;
    description: string | null;
    created_at: string;
    parent_sandbox: string | null;
    status: SandboxStatus;
  };

  type DiffEntry = {
    field: string;
    trunk_assertion_id: string | null;
    trunk_value: unknown;
    sandbox_assertion_id: string | null;
    sandbox_value: unknown;
  };

  let sandboxes: Sandbox[] = [];
  let loading = false;
  let message = '';
  let error = '';

  // Create form
  let showCreateForm = false;
  let newName = '';
  let newDescription = '';
  let creating = false;

  // Diff panel
  let selectedSandbox: Sandbox | null = null;
  let diffEntityId = '';
  let diffEntityType = 'person';
  let diffs: DiffEntry[] = [];
  let loadingDiff = false;
  let diffError = '';

  // Confirm promote/discard
  let confirmAction: { sandbox: Sandbox; action: 'promoted' | 'discarded' } | null = null;
  let actioning = false;

  async function fetchSandboxes(): Promise<void> {
    loading = true;
    error = '';
    try {
      sandboxes = await api.get<Sandbox[]>('/api/v1/sandboxes/');
    } catch (err) {
      error = err instanceof ApiError ? err.message : 'Failed to load sandboxes';
    } finally {
      loading = false;
    }
  }

  async function createSandbox(): Promise<void> {
    if (!newName.trim()) return;
    creating = true;
    error = '';
    try {
      await api.post<Sandbox>('/api/v1/sandboxes/', {
        name: newName.trim(),
        description: newDescription.trim() || null
      });
      message = `Sandbox "${newName}" created.`;
      newName = '';
      newDescription = '';
      showCreateForm = false;
      await fetchSandboxes();
    } catch (err) {
      error = err instanceof ApiError ? err.message : 'Create failed';
    } finally {
      creating = false;
    }
  }

  async function doAction(): Promise<void> {
    if (!confirmAction) return;
    actioning = true;
    error = '';
    try {
      await api.put<void>(`/api/v1/sandboxes/${confirmAction.sandbox.id}/status`, {
        status: confirmAction.action
      });
      message = `Sandbox "${confirmAction.sandbox.name}" ${confirmAction.action}.`;
      if (appState.activeSandboxId === confirmAction.sandbox.id) {
        setSandboxMode(false);
      }
      await fetchSandboxes();
    } catch (err) {
      error = err instanceof ApiError ? err.message : 'Action failed';
    } finally {
      actioning = false;
      confirmAction = null;
    }
  }

  async function deleteSandbox(sandbox: Sandbox): Promise<void> {
    error = '';
    try {
      await api.del<void>(`/api/v1/sandboxes/${sandbox.id}`);
      message = `Deleted sandbox "${sandbox.name}".`;
      if (appState.activeSandboxId === sandbox.id) {
        setSandboxMode(false);
      }
      if (selectedSandbox?.id === sandbox.id) {
        selectedSandbox = null;
      }
      await fetchSandboxes();
    } catch (err) {
      error = err instanceof ApiError ? err.message : 'Delete failed';
    }
  }

  function activateSandbox(sandbox: Sandbox): void {
    setSandboxMode(true, sandbox.id);
    message = `Sandbox "${sandbox.name}" is now active. API calls will include sandbox overlay.`;
  }

  function deactivateSandbox(): void {
    setSandboxMode(false);
    message = 'Switched back to trunk (main) data.';
  }

  async function loadDiff(): Promise<void> {
    if (!selectedSandbox || !diffEntityId.trim()) return;
    loadingDiff = true;
    diffError = '';
    diffs = [];
    try {
      const params = new URLSearchParams({
        entity_id: diffEntityId.trim(),
        entity_type: diffEntityType
      });
      diffs = await api.get<DiffEntry[]>(
        `/api/v1/sandboxes/${selectedSandbox.id}/diff?${params}`
      );
    } catch (err) {
      diffError = err instanceof ApiError ? err.message : 'Diff failed';
    } finally {
      loadingDiff = false;
    }
  }

  function statusBadgeClass(status: SandboxStatus): string {
    if (status === 'active') return 'badge-active';
    if (status === 'promoted') return 'badge-promoted';
    return 'badge-discarded';
  }

  function jsonPreview(val: unknown): string {
    if (val === null || val === undefined) return '—';
    if (typeof val === 'string') return val;
    return JSON.stringify(val);
  }

  onMount(fetchSandboxes);
</script>

<svelte:head>
  <title>Sandboxes</title>
</svelte:head>

<h1>Research Sandboxes</h1>
<p class="subtitle">
  Create hypothesis branches to explore "what-if" scenarios without affecting trunk data. Promote
  or discard when done.
</p>

{#if message}
  <p class="success">{message}</p>
{/if}
{#if error}
  <p class="error">{error}</p>
{/if}

{#if appState.sandboxMode && appState.activeSandboxId}
  <div class="active-banner">
    <strong>Sandbox active:</strong>
    {sandboxes.find((s) => s.id === appState.activeSandboxId)?.name ?? appState.activeSandboxId}
    <button type="button" class="small" on:click={deactivateSandbox}>Switch to trunk</button>
  </div>
{/if}

<section>
  <div class="section-header">
    <h2>Sandboxes</h2>
    <button
      type="button"
      on:click={() => {
        showCreateForm = !showCreateForm;
      }}
    >
      {showCreateForm ? 'Cancel' : '+ New Sandbox'}
    </button>
  </div>

  {#if showCreateForm}
    <div class="form-card">
      <label>
        Name
        <input type="text" bind:value={newName} placeholder="e.g. Hypothesis: Jones family merge" />
      </label>
      <label>
        Description (optional)
        <input type="text" bind:value={newDescription} placeholder="Brief description" />
      </label>
      <button type="button" on:click={createSandbox} disabled={creating || !newName.trim()}>
        {creating ? 'Creating…' : 'Create Sandbox'}
      </button>
    </div>
  {/if}

  {#if loading}
    <p>Loading…</p>
  {:else if sandboxes.length === 0}
    <p>No sandboxes yet.</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Name</th>
          <th>Status</th>
          <th>Created</th>
          <th>Actions</th>
        </tr>
      </thead>
      <tbody>
        {#each sandboxes as sandbox (sandbox.id)}
          <tr class:selected-row={selectedSandbox?.id === sandbox.id}>
            <td>
              <button
                type="button"
                class="link"
                on:click={() => {
                  selectedSandbox = selectedSandbox?.id === sandbox.id ? null : sandbox;
                  diffs = [];
                }}
              >
                {sandbox.name}
              </button>
              {#if sandbox.description}
                <div class="desc">{sandbox.description}</div>
              {/if}
            </td>
            <td><span class="badge {statusBadgeClass(sandbox.status)}">{sandbox.status}</span></td>
            <td>{new Date(sandbox.created_at).toLocaleDateString()}</td>
            <td class="actions">
              {#if sandbox.status === 'active'}
                {#if appState.activeSandboxId === sandbox.id}
                  <button type="button" class="small" on:click={deactivateSandbox}
                    >Deactivate</button
                  >
                {:else}
                  <button type="button" class="small" on:click={() => activateSandbox(sandbox)}
                    >Activate</button
                  >
                {/if}
                <button
                  type="button"
                  class="small success"
                  on:click={() => {
                    confirmAction = { sandbox, action: 'promoted' };
                  }}>Promote</button
                >
                <button
                  type="button"
                  class="small danger"
                  on:click={() => {
                    confirmAction = { sandbox, action: 'discarded' };
                  }}>Discard</button
                >
              {:else}
                <button
                  type="button"
                  class="small danger"
                  on:click={() => deleteSandbox(sandbox)}>Delete</button
                >
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</section>

{#if selectedSandbox}
  <section>
    <h2>Diff: <em>{selectedSandbox.name}</em> vs Trunk</h2>
    <p>Enter an entity ID and type to compare sandbox assertions against trunk data.</p>
    <div class="diff-form">
      <label>
        Entity ID
        <input type="text" bind:value={diffEntityId} placeholder="UUID" />
      </label>
      <label>
        Entity Type
        <select bind:value={diffEntityType}>
          <option value="person">Person</option>
          <option value="family">Family</option>
          <option value="event">Event</option>
          <option value="source">Source</option>
          <option value="place">Place</option>
          <option value="repository">Repository</option>
        </select>
      </label>
      <button type="button" on:click={loadDiff} disabled={loadingDiff || !diffEntityId.trim()}>
        {loadingDiff ? 'Loading…' : 'Compare'}
      </button>
    </div>

    {#if diffError}
      <p class="error">{diffError}</p>
    {:else if diffs.length > 0}
      <table>
        <thead>
          <tr>
            <th>Field</th>
            <th>Trunk Value</th>
            <th>Sandbox Value</th>
          </tr>
        </thead>
        <tbody>
          {#each diffs as diff (diff.field)}
            <tr>
              <td><code>{diff.field}</code></td>
              <td class={diff.trunk_value === null ? 'null-cell' : ''}
                >{jsonPreview(diff.trunk_value)}</td
              >
              <td class={diff.sandbox_value === null ? 'null-cell' : ''}
                >{jsonPreview(diff.sandbox_value)}</td
              >
            </tr>
          {/each}
        </tbody>
      </table>
    {:else if !loadingDiff && diffEntityId}
      <p>No differences found.</p>
    {/if}
  </section>
{/if}

<!-- Confirm promote/discard dialog -->
{#if confirmAction}
  <div class="overlay" role="dialog" aria-modal="true">
    <div class="dialog">
      <h3>Confirm {confirmAction.action === 'promoted' ? 'Promote' : 'Discard'}</h3>
      <p>
        {#if confirmAction.action === 'promoted'}
          Promote <strong>{confirmAction.sandbox.name}</strong>? Sandbox assertions will be merged
          into trunk. This cannot be undone.
        {:else}
          Discard <strong>{confirmAction.sandbox.name}</strong>? The sandbox will be marked
          discarded. Its assertions remain but will no longer be applied.
        {/if}
      </p>
      <div class="dialog-actions">
        <button
          type="button"
          class={confirmAction.action === 'promoted' ? 'success' : 'danger'}
          on:click={doAction}
          disabled={actioning}
        >
          {actioning
            ? 'Processing…'
            : confirmAction.action === 'promoted'
              ? 'Yes, promote'
              : 'Yes, discard'}
        </button>
        <button
          type="button"
          on:click={() => {
            confirmAction = null;
          }}
          disabled={actioning}
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

  .section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.75rem;
  }

  .section-header h2 {
    margin: 0;
    font-size: 1.1rem;
  }

  .form-card {
    background: #f9fafb;
    border: 1px solid #e5e7eb;
    border-radius: 0.375rem;
    padding: 1rem;
    margin-bottom: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.875rem;
    font-weight: 500;
  }

  input,
  select {
    padding: 0.375rem 0.625rem;
    border: 1px solid #d1d5db;
    border-radius: 0.375rem;
    font-size: 0.875rem;
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
    flex-wrap: wrap;
  }

  tr.selected-row {
    background: #eff6ff;
  }

  .desc {
    font-size: 0.8rem;
    color: #6b7280;
    margin-top: 0.2rem;
  }

  .null-cell {
    color: #9ca3af;
    font-style: italic;
  }

  .badge {
    padding: 0.2rem 0.5rem;
    border-radius: 9999px;
    font-size: 0.75rem;
    font-weight: 600;
  }

  .badge-active {
    background: #dcfce7;
    color: #15803d;
  }

  .badge-promoted {
    background: #dbeafe;
    color: #1d4ed8;
  }

  .badge-discarded {
    background: #f3f4f6;
    color: #6b7280;
  }

  .active-banner {
    background: #fef9c3;
    border: 1px solid #fbbf24;
    border-radius: 0.375rem;
    padding: 0.5rem 1rem;
    margin-bottom: 1rem;
    display: flex;
    align-items: center;
    gap: 0.75rem;
    font-size: 0.9rem;
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

  button.small {
    padding: 0.2rem 0.6rem;
    font-size: 0.8rem;
  }

  button.link {
    background: none;
    border: none;
    padding: 0;
    color: #2563eb;
    text-decoration: underline;
    cursor: pointer;
    font-size: inherit;
    text-align: left;
  }

  button.danger {
    color: #dc2626;
    border-color: #fca5a5;
  }

  button.danger:hover:not(:disabled) {
    background: #fee2e2;
  }

  button.success {
    color: #15803d;
    border-color: #86efac;
  }

  button.success:hover:not(:disabled) {
    background: #dcfce7;
  }

  .diff-form {
    display: flex;
    gap: 0.75rem;
    align-items: flex-end;
    flex-wrap: wrap;
    margin-bottom: 1rem;
  }

  .diff-form label {
    min-width: 10rem;
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
