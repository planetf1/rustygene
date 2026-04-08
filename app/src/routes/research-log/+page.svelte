<script lang="ts">
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { api } from '$lib/api';

  type EntityType = 'person' | 'family' | 'source' | 'event';
  type Status = 'open' | 'working' | 'closed' | 'abandoned';

  type EntityReference = {
    entity_type: EntityType;
    id: string;
    label?: string;
  };

  type ResearchLogEntry = {
    id: string;
    date: string;
    researcher: string;
    hypothesis: string;
    action_taken: string;
    outcome: string;
    confidence: number;
    entity_references: EntityReference[];
    status: Status;
    created_at: string;
    updated_at: string;
  };

  type ResearchLogRequest = {
    date: string;
    researcher: string;
    hypothesis: string;
    action_taken: string;
    outcome: string;
    confidence: number;
    status: Status;
    entity_references: EntityReference[];
  };

  type FilterState = {
    status: '' | Status;
    entityType: '' | EntityType;
    entityId: string;
    dateFrom: string;
    dateTo: string;
    query: string;
  };

  const STATUS_LABELS: Record<Status, string> = {
    open: 'Open',
    working: 'Working',
    closed: 'Closed',
    abandoned: 'Abandoned'
  };

  const ENTITY_TYPE_LABELS: Record<EntityType, string> = {
    person: 'Person',
    family: 'Family',
    source: 'Source',
    event: 'Event'
  };

  let loading = false;
  let saving = false;
  let deletingId = '';
  let error = '';

  let entries: ResearchLogEntry[] = [];

  let filters: FilterState = {
    status: '',
    entityType: '',
    entityId: '',
    dateFrom: '',
    dateTo: '',
    query: ''
  };

  let editingId = '';
  let showForm = false;
  let form: ResearchLogRequest = emptyForm();

  function emptyForm(): ResearchLogRequest {
    return {
      date: new Date().toISOString().slice(0, 10),
      researcher: '',
      hypothesis: '',
      action_taken: '',
      outcome: '',
      confidence: 0.7,
      status: 'open',
      entity_references: []
    };
  }

  function toInputDate(value: string): string {
    if (!value) {
      return '';
    }
    return value.slice(0, 10);
  }

  function statusClass(status: Status): string {
    return `status-${status}`;
  }

  function routeForEntity(reference: EntityReference): string {
    switch (reference.entity_type) {
      case 'person':
        return `/persons/${reference.id}`;
      case 'family':
        return `/families/${reference.id}`;
      case 'source':
        return `/sources/${reference.id}`;
      case 'event':
        return `/events/${reference.id}`;
      default:
        return '/research-log';
    }
  }

  function formatDate(value: string): string {
    if (!value) {
      return 'Unknown date';
    }

    const date = new Date(value);
    if (Number.isNaN(date.getTime())) {
      return value;
    }

    return date.toLocaleDateString();
  }

  function parseConfidence(value: string): number {
    const parsed = Number.parseFloat(value);
    if (Number.isNaN(parsed)) {
      return 0;
    }
    return Math.max(0, Math.min(1, parsed));
  }

  async function loadEntries(): Promise<void> {
    loading = true;
    error = '';
    try {
      const params = new URLSearchParams();
      params.set('limit', '500');
      params.set('offset', '0');

      if (filters.status) {
        params.set('status', filters.status);
      }
      if (filters.entityType) {
        params.set('entity_type', filters.entityType);
      }
      if (filters.entityId.trim()) {
        params.set('entity_id', filters.entityId.trim());
      }
      if (filters.dateFrom) {
        params.set('date_from', filters.dateFrom);
      }
      if (filters.dateTo) {
        params.set('date_to', filters.dateTo);
      }
      if (filters.query.trim()) {
        params.set('q', filters.query.trim());
      }

      entries = await api.get<ResearchLogEntry[]>(`/api/v1/research-log?${params.toString()}`);
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load research log entries';
    } finally {
      loading = false;
    }
  }

  function openCreate(): void {
    editingId = '';
    form = emptyForm();
    showForm = true;
    error = '';
  }

  function openEdit(entry: ResearchLogEntry): void {
    editingId = entry.id;
    form = {
      date: toInputDate(entry.date),
      researcher: entry.researcher,
      hypothesis: entry.hypothesis,
      action_taken: entry.action_taken,
      outcome: entry.outcome,
      confidence: entry.confidence,
      status: entry.status,
      entity_references: [...entry.entity_references]
    };
    showForm = true;
    error = '';
  }

  function addEntityReference(): void {
    form = {
      ...form,
      entity_references: [...form.entity_references, { entity_type: 'person', id: '' }]
    };
  }

  function removeEntityReference(index: number): void {
    form = {
      ...form,
      entity_references: form.entity_references.filter((_, idx) => idx !== index)
    };
  }

  function updateEntityReference(index: number, next: EntityReference): void {
    form = {
      ...form,
      entity_references: form.entity_references.map((current, idx) => (idx === index ? next : current))
    };
  }

  async function saveEntry(): Promise<void> {
    if (!form.hypothesis.trim()) {
      error = 'Hypothesis or research question is required.';
      return;
    }

    saving = true;
    error = '';
    try {
      const payload: ResearchLogRequest = {
        ...form,
        hypothesis: form.hypothesis.trim(),
        researcher: form.researcher.trim(),
        action_taken: form.action_taken.trim(),
        outcome: form.outcome.trim(),
        confidence: Math.max(0, Math.min(1, form.confidence)),
        entity_references: form.entity_references.filter((ref) => ref.id.trim().length > 0)
      };

      if (editingId) {
        await api.put(`/api/v1/research-log/${editingId}`, payload);
      } else {
        await api.post('/api/v1/research-log', payload);
      }

      showForm = false;
      await loadEntries();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to save entry';
    } finally {
      saving = false;
    }
  }

  async function deleteEntry(id: string): Promise<void> {
    if (!confirm('Delete this research log entry?')) {
      return;
    }

    deletingId = id;
    error = '';
    try {
      await api.del(`/api/v1/research-log/${id}`);
      await loadEntries();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to delete entry';
    } finally {
      deletingId = '';
    }
  }

  onMount(() => {
    const entityType = $page.url.searchParams.get('entityType');
    const query = $page.url.searchParams.get('query');

    if (entityType && ['person', 'family', 'source', 'event'].includes(entityType)) {
      filters = {
        ...filters,
        entityType: entityType as EntityType,
        entityId: query ?? ''
      };
    }

    void loadEntries();
  });
</script>

<main class="panel">
  <header class="header">
    <div>
      <h1>Research Log</h1>
      <p>Track hypotheses, actions, outcomes, and linked entities in one workflow.</p>
    </div>
    <button type="button" class="primary" on:click={openCreate}>New entry</button>
  </header>

  <section class="filters">
    <select bind:value={filters.status}>
      <option value="">All statuses</option>
      <option value="open">Open</option>
      <option value="working">Working</option>
      <option value="closed">Closed</option>
      <option value="abandoned">Abandoned</option>
    </select>

    <select bind:value={filters.entityType}>
      <option value="">All entity types</option>
      <option value="person">Person</option>
      <option value="family">Family</option>
      <option value="source">Source</option>
      <option value="event">Event</option>
    </select>

    <input type="text" bind:value={filters.entityId} placeholder="Entity ID" />

    <input type="date" bind:value={filters.dateFrom} aria-label="Filter from date" />
    <input type="date" bind:value={filters.dateTo} aria-label="Filter to date" />
    <input type="search" bind:value={filters.query} placeholder="Search hypothesis, action, outcome, researcher" />

    <button type="button" class="secondary" on:click={loadEntries}>Apply</button>
    <button
      type="button"
      class="secondary"
      on:click={() => {
        filters = { status: '', entityType: '', entityId: '', dateFrom: '', dateTo: '', query: '' };
        void loadEntries();
      }}
    >
      Reset
    </button>
  </section>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if loading}
    <p class="muted">Loading entries…</p>
  {:else if entries.length === 0}
    <p class="muted">No research log entries matched the current filters.</p>
  {:else}
    <section class="entries">
      {#each entries as entry (entry.id)}
        <article class="entry-card">
          <header class="entry-head">
            <div>
              <h2>{entry.hypothesis}</h2>
              <p class="meta">
                <span>{formatDate(entry.date)}</span>
                <span>•</span>
                <span>{entry.researcher || 'Unassigned researcher'}</span>
                <span>•</span>
                <span>Confidence {(entry.confidence * 100).toFixed(0)}%</span>
              </p>
            </div>
            <span class={`status ${statusClass(entry.status)}`}>{STATUS_LABELS[entry.status]}</span>
          </header>

          {#if entry.action_taken}
            <p><strong>Action:</strong> {entry.action_taken}</p>
          {/if}
          {#if entry.outcome}
            <p><strong>Outcome:</strong> {entry.outcome}</p>
          {/if}

          <div class="chips">
            {#if entry.entity_references.length === 0}
              <span class="chip chip-empty">No linked entities</span>
            {:else}
              {#each entry.entity_references as reference}
                <button type="button" class="chip" on:click={() => goto(routeForEntity(reference))}>
                  {ENTITY_TYPE_LABELS[reference.entity_type]}: {reference.label || reference.id}
                </button>
              {/each}
            {/if}
          </div>

          <div class="entry-actions">
            <button type="button" class="secondary" on:click={() => openEdit(entry)}>Edit</button>
            <button type="button" class="danger" on:click={() => deleteEntry(entry.id)} disabled={deletingId === entry.id}>
              {deletingId === entry.id ? 'Deleting…' : 'Delete'}
            </button>
          </div>
        </article>
      {/each}
    </section>
  {/if}

  {#if showForm}
    <button type="button" class="overlay" aria-label="Close form" on:click={() => (showForm = false)}></button>
    <aside class="slideover">
      <h2>{editingId ? 'Edit research log entry' : 'Create research log entry'}</h2>

      <label>
        Date
        <input type="date" bind:value={form.date} />
      </label>

      <label>
        Researcher
        <input type="text" bind:value={form.researcher} placeholder="Researcher name" />
      </label>

      <label>
        Hypothesis / Question
        <textarea rows={3} bind:value={form.hypothesis} placeholder="What are you trying to prove or disprove?"></textarea>
      </label>

      <label>
        Action taken
        <textarea rows={3} bind:value={form.action_taken} placeholder="Searches, repositories, interviews, etc."></textarea>
      </label>

      <label>
        Outcome
        <textarea rows={3} bind:value={form.outcome} placeholder="What was found? What remains uncertain?"></textarea>
      </label>

      <div class="inline-grid">
        <label>
          Status
          <select bind:value={form.status}>
            <option value="open">Open</option>
            <option value="working">Working</option>
            <option value="closed">Closed</option>
            <option value="abandoned">Abandoned</option>
          </select>
        </label>

        <label>
          Confidence (0–1)
          <input
            type="number"
            min="0"
            max="1"
            step="0.01"
            value={form.confidence}
            on:input={(event) => {
              form = { ...form, confidence: parseConfidence((event.currentTarget as HTMLInputElement).value) };
            }}
          />
        </label>
      </div>

      <section class="entity-editor">
        <h3>Linked entities</h3>
        {#if form.entity_references.length === 0}
          <p class="muted">No linked entities yet.</p>
        {:else}
          {#each form.entity_references as reference, index}
            <div class="entity-row">
              <select
                value={reference.entity_type}
                on:change={(event) => {
                  updateEntityReference(index, {
                    ...reference,
                    entity_type: (event.currentTarget as HTMLSelectElement).value as EntityType
                  });
                }}
              >
                <option value="person">Person</option>
                <option value="family">Family</option>
                <option value="source">Source</option>
                <option value="event">Event</option>
              </select>
              <input
                type="text"
                value={reference.id}
                placeholder="Entity ID"
                on:input={(event) => {
                  updateEntityReference(index, {
                    ...reference,
                    id: (event.currentTarget as HTMLInputElement).value
                  });
                }}
              />
              <input
                type="text"
                value={reference.label ?? ''}
                placeholder="Label (optional)"
                on:input={(event) => {
                  updateEntityReference(index, {
                    ...reference,
                    label: (event.currentTarget as HTMLInputElement).value
                  });
                }}
              />
              <button type="button" class="danger" on:click={() => removeEntityReference(index)}>Remove</button>
            </div>
          {/each}
        {/if}
        <button type="button" class="secondary" on:click={addEntityReference}>Add linked entity</button>
      </section>

      <div class="form-actions">
        <button type="button" class="primary" on:click={saveEntry} disabled={saving}>{saving ? 'Saving…' : 'Save entry'}</button>
        <button type="button" class="secondary" on:click={() => (showForm = false)}>Cancel</button>
      </div>
    </aside>
  {/if}
</main>

<style>
  .panel {
    background: linear-gradient(180deg, #ffffff 0%, #fff9ff 100%);
    border: 1px solid #e8def8;
    border-radius: 1rem;
    padding: 1.2rem;
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 0.8rem;
  }

  h1,
  h2,
  h3,
  p {
    margin: 0;
  }

  .filters {
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
    gap: 0.5rem;
  }

  .entries {
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
  }

  .entry-card {
    border: 1px solid #e9dcff;
    border-radius: 0.8rem;
    padding: 0.75rem;
    background: #fff;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
  }

  .entry-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 0.6rem;
  }

  .meta {
    color: #685b8f;
    font-size: 0.85rem;
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    flex-wrap: wrap;
  }

  .status {
    border-radius: 999px;
    padding: 0.15rem 0.55rem;
    font-size: 0.75rem;
    font-weight: 600;
    border: 1px solid transparent;
  }

  .status-open {
    background: #eef2ff;
    color: #4338ca;
    border-color: #c7d2fe;
  }

  .status-working {
    background: #fff7ed;
    color: #9a3412;
    border-color: #fdba74;
  }

  .status-closed {
    background: #f0fdf4;
    color: #166534;
    border-color: #86efac;
  }

  .status-abandoned {
    background: #fef2f2;
    color: #991b1b;
    border-color: #fca5a5;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }

  .chip {
    background: #f8f1ff;
    border: 1px solid #dfd2f8;
    color: #5b21b6;
    border-radius: 999px;
    padding: 0.15rem 0.55rem;
    font-size: 0.78rem;
    cursor: pointer;
  }

  .chip-empty {
    cursor: default;
  }

  .entry-actions {
    display: flex;
    gap: 0.4rem;
  }

  input,
  textarea,
  select {
    border: 1px solid #dfd2f8;
    border-radius: 0.65rem;
    padding: 0.45rem 0.6rem;
    font: inherit;
  }

  textarea {
    resize: vertical;
    min-height: 4rem;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    color: #3c2f63;
    font-size: 0.88rem;
  }

  .inline-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.5rem;
  }

  .entity-editor {
    border: 1px solid #efe6ff;
    border-radius: 0.8rem;
    padding: 0.6rem;
    background: #fffdff;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
  }

  .entity-row {
    display: grid;
    grid-template-columns: 10rem 1fr 1fr auto;
    gap: 0.4rem;
    align-items: center;
  }

  .form-actions {
    display: flex;
    gap: 0.45rem;
  }

  button {
    border: 0;
    border-radius: 0.65rem;
    padding: 0.38rem 0.7rem;
    cursor: pointer;
    font: inherit;
  }

  .primary {
    background: #6d28d9;
    color: #fff;
  }

  .primary:hover {
    background: #5b21b6;
  }

  .secondary {
    background: #f3edff;
    color: #5b21b6;
    border: 1px solid #dfd2f8;
  }

  .danger {
    background: #fef2f2;
    color: #991b1b;
    border: 1px solid #fca5a5;
  }

  .muted {
    color: #6b6192;
  }

  .error {
    color: #b91c1c;
  }

  .overlay {
    position: fixed;
    inset: 0;
    background: rgb(52 32 97 / 32%);
    border: 0;
    width: 100%;
    padding: 0;
    border-radius: 0;
    cursor: default;
  }

  .slideover {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(760px, 100%);
    background: #ffffff;
    border-left: 1px solid #e8def8;
    padding: 1rem;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
  }

  @media (max-width: 1180px) {
    .filters {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .entity-row {
      grid-template-columns: 1fr;
    }
  }
</style>
