<script lang="ts">
  import { api, ApiError } from '$lib/api';

  type MergeSelection = {
    entity_type: string;
    entity_id: string;
    field: string;
    new_value: unknown;
    source?: string | null;
    confidence?: number | null;
  };

  type MergeDiffFieldPreview = {
    entity_id: string;
    field: string;
    old_value: unknown;
    new_value: unknown;
    source: string;
    confidence: number;
  };

  type MergeNewEntityPreview = {
    entity_id: string;
    label: string;
    xref: string | null;
    fields: MergeSelection[];
  };

  type MergeDiffResponse = {
    changed_fields: MergeDiffFieldPreview[];
    new_entities: MergeNewEntityPreview[];
    unchanged_entities: number;
  };

  type ImportMergeResponse = {
    proposals_created: number;
    proposal_ids: string[];
  };

  let selectedFile: File | null = null;
  let selectedFormat = 'gedcom';
  let busy = false;
  let loadingDiff = false;
  let applying = false;
  let error = '';
  let info = '';

  let diff: MergeDiffResponse | null = null;
  let selectedChangedKeys = new Set<string>();
  let selectedNewKeys = new Set<string>();

  function handleFile(event: Event): void {
    const target = event.currentTarget as HTMLInputElement;
    selectedFile = target.files?.[0] ?? null;
    diff = null;
    selectedChangedKeys = new Set();
    selectedNewKeys = new Set();
    error = '';
    info = '';
  }

  function keyForChanged(item: MergeDiffFieldPreview): string {
    return `${item.entity_id}|${item.field}|${JSON.stringify(item.new_value)}`;
  }

  function keyForNew(item: MergeSelection): string {
    return `${item.entity_id}|${item.field}|${JSON.stringify(item.new_value)}`;
  }

  function toggleChanged(item: MergeDiffFieldPreview): void {
    const key = keyForChanged(item);
    if (selectedChangedKeys.has(key)) selectedChangedKeys.delete(key);
    else selectedChangedKeys.add(key);
    selectedChangedKeys = new Set(selectedChangedKeys);
  }

  function toggleNew(item: MergeSelection): void {
    const key = keyForNew(item);
    if (selectedNewKeys.has(key)) selectedNewKeys.delete(key);
    else selectedNewKeys.add(key);
    selectedNewKeys = new Set(selectedNewKeys);
  }

  async function previewDiff(): Promise<void> {
    if (!selectedFile) {
      error = 'Select a GEDCOM file first.';
      return;
    }

    loadingDiff = true;
    busy = true;
    error = '';
    info = '';

    try {
      const formData = new FormData();
      formData.set('format', selectedFormat);
      formData.set('file', selectedFile, selectedFile.name);
      diff = await api.postFormData<MergeDiffResponse>('/api/v1/import/diff', formData);

      selectedChangedKeys = new Set(diff.changed_fields.map((item) => keyForChanged(item)));
      selectedNewKeys = new Set();

      info = `Diff ready: ${diff.changed_fields.length} changed fields, ${diff.new_entities.length} new entities, ${diff.unchanged_entities} unchanged.`;
    } catch (err) {
      error = err instanceof ApiError ? err.message : 'Failed to compute import diff';
      diff = null;
    } finally {
      loadingDiff = false;
      busy = false;
    }
  }

  async function applySelected(): Promise<void> {
    if (!diff) {
      error = 'Generate a diff first.';
      return;
    }

    const selected_changes: MergeSelection[] = [];

    for (const item of diff.changed_fields) {
      if (selectedChangedKeys.has(keyForChanged(item))) {
        selected_changes.push({
          entity_type: 'person',
          entity_id: item.entity_id,
          field: item.field,
          new_value: item.new_value,
          source: item.source,
          confidence: item.confidence
        });
      }
    }

    for (const entity of diff.new_entities) {
      for (const field of entity.fields) {
        if (selectedNewKeys.has(keyForNew(field))) {
          selected_changes.push(field);
        }
      }
    }

    if (selected_changes.length === 0) {
      error = 'Select at least one change to submit.';
      return;
    }

    applying = true;
    busy = true;
    error = '';

    try {
      const response = await api.post<ImportMergeResponse>('/api/v1/import/merge', {
        selected_changes,
        submitted_by: 'ui:import-merge'
      });

      info = `Submitted ${response.proposals_created} staging proposal(s).`;
    } catch (err) {
      error = err instanceof ApiError ? err.message : 'Failed to submit selected merge changes';
    } finally {
      applying = false;
      busy = false;
    }
  }

  function toPretty(value: unknown): string {
    if (typeof value === 'string') return value;
    return JSON.stringify(value);
  }
</script>

<svelte:head>
  <title>Import Merge Review</title>
</svelte:head>

<main class="panel">
  <h1>GEDCOM selective merge review</h1>
  <p class="subtitle">Upload an updated GEDCOM, review detected changes, and submit only selected items to the staging queue.</p>

  <section class="card">
    <label>
      GEDCOM file
      <input type="file" accept=".ged" on:change={handleFile} disabled={busy} />
    </label>

    <div class="actions">
      <button type="button" on:click={previewDiff} disabled={loadingDiff || !selectedFile}>
        {loadingDiff ? 'Computing…' : 'Preview diff'}
      </button>
      <button type="button" class="secondary" on:click={applySelected} disabled={applying || !diff}>
        {applying ? 'Submitting…' : 'Submit selected to staging'}
      </button>
    </div>
  </section>

  {#if diff}
    <section class="card">
      <h2>Changed fields</h2>
      {#if diff.changed_fields.length === 0}
        <p class="muted">No changed fields detected.</p>
      {:else}
        <table>
          <thead>
            <tr>
              <th>Select</th>
              <th>Entity</th>
              <th>Field</th>
              <th>Old</th>
              <th>New</th>
            </tr>
          </thead>
          <tbody>
            {#each diff.changed_fields as item}
              <tr>
                <td>
                  <input
                    type="checkbox"
                    checked={selectedChangedKeys.has(keyForChanged(item))}
                    on:change={() => toggleChanged(item)}
                  />
                </td>
                <td class="mono">{item.entity_id}</td>
                <td>{item.field}</td>
                <td>{toPretty(item.old_value)}</td>
                <td>{toPretty(item.new_value)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </section>

    <section class="card">
      <h2>New entities</h2>
      {#if diff.new_entities.length === 0}
        <p class="muted">No new entities detected.</p>
      {:else}
        {#each diff.new_entities as entity}
          <article class="entity-card">
            <h3>{entity.label}</h3>
            <p class="muted">{entity.entity_id} {entity.xref ? `(${entity.xref})` : ''}</p>
            <table>
              <thead>
                <tr>
                  <th>Select</th>
                  <th>Field</th>
                  <th>Value</th>
                </tr>
              </thead>
              <tbody>
                {#each entity.fields as field}
                  <tr>
                    <td>
                      <input
                        type="checkbox"
                        checked={selectedNewKeys.has(keyForNew(field))}
                        on:change={() => toggleNew(field)}
                      />
                    </td>
                    <td>{field.field}</td>
                    <td>{toPretty(field.new_value)}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </article>
        {/each}
      {/if}
    </section>
  {/if}

  {#if info}
    <p class="ok">{info}</p>
  {/if}
  {#if error}
    <p class="error">{error}</p>
  {/if}
</main>

<style>
  .panel { display: flex; flex-direction: column; gap: 1rem; }
  .subtitle, .muted { color: #475569; margin: 0; }
  .card { border: 1px solid #e2e8f0; border-radius: 0.65rem; padding: 1rem; display: flex; flex-direction: column; gap: 0.75rem; }
  .actions { display: flex; gap: 0.6rem; flex-wrap: wrap; }
  button { border: 0; border-radius: 0.45rem; padding: 0.5rem 0.85rem; background: #2563eb; color: #fff; cursor: pointer; }
  button.secondary { background: #e2e8f0; color: #0f172a; }
  button:disabled { opacity: 0.6; cursor: not-allowed; }
  table { width: 100%; border-collapse: collapse; }
  th, td { padding: 0.45rem; border-bottom: 1px solid #e2e8f0; text-align: left; vertical-align: top; }
  .mono { font-family: ui-monospace, Menlo, Consolas, monospace; font-size: 0.85rem; }
  .entity-card { border: 1px solid #e2e8f0; border-radius: 0.55rem; padding: 0.75rem; }
  .ok { color: #166534; }
  .error { color: #b91c1c; }
</style>
