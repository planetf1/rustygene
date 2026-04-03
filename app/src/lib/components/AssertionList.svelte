<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { api } from '$lib/api';
  import CitationDetail from '$lib/components/CitationDetail.svelte';

  type CitationRef = { citation_id?: string; source_id?: string; note?: string };

  type AssertionItem = {
    assertion_id: string;
    status: string;
    confidence: number;
    evidence_type?: string;
    sources: CitationRef[];
    value: unknown;
    created_at?: string;
    updated_at?: string;
  };

  export let entityId: string;
  export let entityType: 'persons' | 'families' | 'events' = 'persons';
  export let assertions: Record<string, AssertionItem[]> = {};

  const dispatch = createEventDispatcher<{ updated: void }>();

  let activeCompareField: string | null = null;
  let compareLeftId: string | null = null;
  let compareRightId: string | null = null;
  let activeCitationId = '';
  let activeCitationNote = '';
  let busyAssertionId = '';
  let error = '';

  // New observation form
  let showAddForm = false;
  let newField = '';
  let newValue = '';
  let newEvidenceType: 'direct' | 'indirect' | 'negative' = 'direct';
  let addingObservation = false;
  let addError = '';

  function rowsFor(field: string): AssertionItem[] {
    return [...(assertions[field] ?? [])].sort((a, b) => b.confidence - a.confidence);
  }

  function formatValue(value: unknown): string {
    if (value === null || value === undefined) {
      return '—';
    }

    if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') {
      return String(value);
    }

    try {
      return JSON.stringify(value, null, 2);
    } catch {
      return String(value);
    }
  }

  function competingCount(field: string): number {
    const count = rowsFor(field).length;
    return count > 1 ? count : 0;
  }

  function statusTone(status: string): string {
    const normalized = status.toLowerCase();
    if (normalized.includes('confirm') || normalized.includes('approv')) {
      return 'ok';
    }
    if (normalized.includes('proposed')) {
      return 'pending';
    }
    if (normalized.includes('disput')) {
      return 'warn';
    }
    if (normalized.includes('reject') || normalized.includes('retract')) {
      return 'bad';
    }
    return 'neutral';
  }

  function chooseCompare(field: string): void {
    const rows = rowsFor(field);
    if (rows.length < 2) {
      return;
    }

    activeCompareField = field;
    compareLeftId = rows[0].assertion_id;
    compareRightId = rows[1].assertion_id;
  }

  function assertionById(field: string, assertionId: string | null): AssertionItem | null {
    if (!assertionId) {
      return null;
    }

    return rowsFor(field).find((row) => row.assertion_id === assertionId) ?? null;
  }

  async function updateAssertion(assertionId: string, payload: unknown): Promise<void> {
    error = '';
    busyAssertionId = assertionId;

    try {
      const endpoint =
        entityType === 'persons'
          ? `/api/v1/persons/${entityId}/assertions/${assertionId}`
          : `/api/v1/assertions/${assertionId}`;
      await api.put(endpoint, payload);
      dispatch('updated');
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to update assertion';
    } finally {
      busyAssertionId = '';
    }
  }

  async function setPreferred(assertionId: string, field: string): Promise<void> {
    await updateAssertion(assertionId, { preferred: true });
  }

  async function changeConfidence(assertionId: string, field: string, nextConfidence: number): Promise<void> {
    const clamped = Math.max(0, Math.min(1, nextConfidence));
    await updateAssertion(assertionId, { confidence: clamped });
  }

  async function markStatus(assertionId: string, field: string, status: 'disputed' | 'rejected'): Promise<void> {
    const reason = prompt(`Reason for ${status}:`)?.trim() ?? '';
    if (!reason) {
      return;
    }

    await updateAssertion(assertionId, { status, reason });
  }

  async function approveProposal(assertionId: string, field: string): Promise<void> {
    await updateAssertion(assertionId, { status: 'confirmed', preferred: true });
  }

  async function attachCitation(assertionId: string, field: string): Promise<void> {
    const sourceId = prompt('Source ID to link as citation:')?.trim() ?? '';
    if (!sourceId) {
      return;
    }

    const note = prompt('Optional citation note:')?.trim() ?? undefined;

    error = '';
    busyAssertionId = assertionId;

    try {
      await api.post('/api/v1/citations', {
        source_id: sourceId,
        assertion_id: assertionId,
        citation_note: note
      });
      dispatch('updated');
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to attach citation';
    } finally {
      busyAssertionId = '';
    }
  }

  function openCitation(citationId: string, citationNote: string | undefined): void {
    activeCitationId = citationId;
    activeCitationNote = citationNote ?? '';
  }

  function evidenceBadgeClass(evidenceType: string | undefined): string {
    switch (evidenceType) {
      case 'negative': return 'evidence-negative';
      case 'indirect': return 'evidence-indirect';
      default: return 'evidence-direct';
    }
  }

  async function addObservation(): Promise<void> {
    if (!newField.trim()) {
      addError = 'Field name is required';
      return;
    }
    addingObservation = true;
    addError = '';
    try {
      const endpoint = `/api/v1/${entityType}/${entityId}/assertions`;
      await api.post(endpoint, {
        field: newField.trim(),
        value: newValue.trim() || null,
        evidence_type: newEvidenceType,
        status: 'proposed'
      });
      newField = '';
      newValue = '';
      newEvidenceType = 'direct';
      showAddForm = false;
      dispatch('updated');
    } catch (err) {
      addError = err instanceof Error ? err.message : 'Failed to add observation';
    } finally {
      addingObservation = false;
    }
  }
</script>

<section class="assertions">
  <header>
    <h2>Assertions</h2>
    <p>Preferred assertion is listed first for each field.</p>
    <button type="button" class="secondary" on:click={() => (showAddForm = !showAddForm)}>
      {showAddForm ? 'Cancel' : '+ Add observation'}
    </button>
  </header>

  {#if showAddForm}
    <form class="add-form" on:submit|preventDefault={addObservation}>
      <label>
        Field
        <input type="text" bind:value={newField} placeholder="e.g. occupation, note" />
      </label>
      <label>
        Value
        <input type="text" bind:value={newValue} placeholder="e.g. Carpenter (or blank for Negative)" />
      </label>
      <label>
        Evidence type
        <select bind:value={newEvidenceType}>
          <option value="direct">Direct</option>
          <option value="indirect">Indirect</option>
          <option value="negative">Negative (searched and did NOT find)</option>
        </select>
      </label>
      {#if newEvidenceType === 'negative'}
        <p class="negative-hint">Describe in the value what you searched and did not find.</p>
      {/if}
      {#if addError}
        <p class="error">{addError}</p>
      {/if}
      <button type="submit" disabled={addingObservation}>{addingObservation ? 'Saving…' : 'Save observation'}</button>
    </form>
  {/if}

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if Object.keys(assertions).length === 0}
    <p>No assertions found.</p>
  {:else}
    {#each Object.keys(assertions).sort() as field}
      <article class="field-block">
        <div class="field-head">
          <h3>{field}</h3>
          {#if competingCount(field) > 0}
            <div class="competing">
              <span>{competingCount(field)} competing assertions</span>
              <button type="button" class="secondary" on:click={() => chooseCompare(field)}>Compare</button>
            </div>
          {/if}
        </div>

        <div class="cards">
          {#each rowsFor(field) as assertion, idx}
            <section class={`card ${idx === 0 ? 'preferred' : 'non-preferred'}`}>
              <div class="meta">
                <span class={`status ${statusTone(assertion.status)}`}>{assertion.status}</span>
                <span class="confidence">{assertion.confidence.toFixed(2)}</span>
                <span class={`evidence-badge ${evidenceBadgeClass(assertion.evidence_type)}`}>{assertion.evidence_type ?? 'direct'}</span>
                {#if idx === 0}
                  <span class="badge">Preferred</span>
                {/if}
              </div>

              <pre>{formatValue(assertion.value)}</pre>

              <div class="citations">
                {#if assertion.sources.length === 0}
                  <span>No citations</span>
                {:else}
                  {#each assertion.sources as source}
                    {#if source.citation_id}
                      <button
                        type="button"
                        class="citation-chip"
                        on:click={() => openCitation(source.citation_id ?? '', source.note)}
                      >
                        {source.citation_id}
                      </button>
                    {:else}
                      <code>{source.source_id ?? 'citation'}</code>
                    {/if}
                  {/each}
                {/if}
              </div>

              {#if activeCitationId && assertion.sources.some((source) => source.citation_id === activeCitationId)}
                <CitationDetail citationId={activeCitationId} citationNote={activeCitationNote} />
              {/if}

              <div class="actions">
                <button
                  type="button"
                  class="secondary"
                  disabled={busyAssertionId === assertion.assertion_id}
                  on:click={() => setPreferred(assertion.assertion_id, field)}
                >Set as preferred</button>

                {#if assertion.status.toLowerCase().includes('proposed')}
                  <button
                    type="button"
                    disabled={busyAssertionId === assertion.assertion_id}
                    on:click={() => approveProposal(assertion.assertion_id, field)}
                  >Approve</button>
                {/if}

                <button
                  type="button"
                  class="secondary"
                  disabled={busyAssertionId === assertion.assertion_id}
                  on:click={() => markStatus(assertion.assertion_id, field, 'disputed')}
                >Dispute</button>

                <button
                  type="button"
                  class="danger"
                  disabled={busyAssertionId === assertion.assertion_id}
                  on:click={() => markStatus(assertion.assertion_id, field, 'rejected')}
                >Reject</button>

                <label>
                  Confidence
                  <input
                    type="number"
                    min="0"
                    max="1"
                    step="0.01"
                    value={assertion.confidence}
                    disabled={busyAssertionId === assertion.assertion_id}
                    on:change={(event) =>
                      changeConfidence(
                        assertion.assertion_id,
                        field,
                        Number((event.currentTarget as HTMLInputElement).value)
                      )}
                  />
                </label>

                <button
                  type="button"
                  class="secondary"
                  disabled={busyAssertionId === assertion.assertion_id}
                  on:click={() => attachCitation(assertion.assertion_id, field)}
                >Attach citation</button>
              </div>

              {#if assertion.created_at || assertion.updated_at}
                <p class="timestamps">
                  Created {assertion.created_at ?? '—'} · Updated {assertion.updated_at ?? '—'}
                </p>
              {/if}
            </section>
          {/each}
        </div>

        {#if activeCompareField === field}
          {@const left = assertionById(field, compareLeftId)}
          {@const right = assertionById(field, compareRightId)}
          {#if left && right}
            <section class="compare">
              <h4>Comparison</h4>
              <div class="compare-grid">
                <article>
                  <h5>A ({left.confidence.toFixed(2)})</h5>
                  <pre>{formatValue(left.value)}</pre>
                  <button type="button" on:click={() => setPreferred(left.assertion_id, field)}>Accept A</button>
                </article>
                <article>
                  <h5>B ({right.confidence.toFixed(2)})</h5>
                  <pre>{formatValue(right.value)}</pre>
                  <button type="button" on:click={() => setPreferred(right.assertion_id, field)}>Accept B</button>
                </article>
              </div>
              <button type="button" class="secondary" on:click={() => (activeCompareField = null)}>Close compare</button>
            </section>
          {/if}
        {/if}
      </article>
    {/each}
  {/if}
</section>

<style>
  .assertions {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  header h2 {
    margin: 0;
  }

  header p {
    margin: 0.25rem 0 0;
    color: #64748b;
  }

  .error {
    color: #b91c1c;
    margin: 0;
  }

  .field-block {
    border: 1px solid #e2e8f0;
    border-radius: 0.65rem;
    padding: 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .field-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .field-head h3 {
    margin: 0;
    text-transform: capitalize;
  }

  .competing {
    display: inline-flex;
    gap: 0.5rem;
    align-items: center;
    color: #334155;
    font-size: 0.9rem;
  }

  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
    gap: 0.75rem;
  }

  .card {
    border-radius: 0.6rem;
    background: #f8fafc;
    padding: 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
  }

  .card.preferred {
    border: 2px solid #2563eb;
  }

  .card.non-preferred {
    border: 2px dashed #94a3b8;
  }

  .meta {
    display: flex;
    align-items: center;
    gap: 0.45rem;
  }

  .status {
    border-radius: 999px;
    padding: 0.1rem 0.5rem;
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
  }

  .status.ok { background: #dcfce7; color: #166534; }
  .status.pending { background: #fef3c7; color: #92400e; }
  .status.warn { background: #fee2e2; color: #9f1239; }
  .status.bad { background: #fee2e2; color: #991b1b; }
  .status.neutral { background: #e2e8f0; color: #334155; }

  .confidence {
    font-weight: 700;
    color: #1e293b;
  }

  .badge {
    margin-left: auto;
    font-size: 0.75rem;
    color: #1e3a8a;
    background: #dbeafe;
    border-radius: 999px;
    padding: 0.1rem 0.45rem;
  }

  .evidence-badge {
    font-size: 0.7rem;
    font-weight: 600;
    text-transform: uppercase;
    border-radius: 999px;
    padding: 0.1rem 0.45rem;
  }

  .evidence-direct { background: #f0fdf4; color: #166534; border: 1px solid #bbf7d0; }
  .evidence-indirect { background: #fef9c3; color: #713f12; border: 1px solid #fde68a; }
  .evidence-negative { background: #fee2e2; color: #991b1b; border: 1px solid #fca5a5; text-decoration: line-through; }

  .add-form {
    border: 1px dashed #2563eb;
    border-radius: 0.65rem;
    padding: 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
    background: #eff6ff;
  }

  .negative-hint {
    margin: 0;
    color: #991b1b;
    font-size: 0.8rem;
    font-style: italic;
  }

  select {
    border: 1px solid #cbd5e1;
    border-radius: 0.35rem;
    padding: 0.2rem 0.4rem;
    font-size: 0.85rem;
  }

  header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  header h2 { margin: 0; flex: 1; }

  pre {
    margin: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    background: #fff;
    border: 1px solid #e2e8f0;
    border-radius: 0.5rem;
    padding: 0.5rem;
    font-size: 0.85rem;
  }

  .citations {
    display: flex;
    gap: 0.35rem;
    flex-wrap: wrap;
    color: #475569;
    font-size: 0.85rem;
  }

  code {
    background: #e2e8f0;
    border-radius: 0.3rem;
    padding: 0.05rem 0.35rem;
  }

  .citation-chip {
    border: 1px solid #93c5fd;
    border-radius: 999px;
    padding: 0.08rem 0.45rem;
    background: #dbeafe;
    color: #1e3a8a;
    font-size: 0.8rem;
    cursor: pointer;
  }

  .actions {
    display: flex;
    gap: 0.4rem;
    flex-wrap: wrap;
    align-items: center;
  }

  button {
    border: 0;
    border-radius: 0.4rem;
    padding: 0.35rem 0.55rem;
    color: #fff;
    background: #2563eb;
    cursor: pointer;
  }

  button.secondary {
    background: #475569;
  }

  button.danger {
    background: #b91c1c;
  }

  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  label {
    display: inline-flex;
    gap: 0.25rem;
    align-items: center;
    color: #334155;
    font-size: 0.82rem;
  }

  input[type='number'] {
    width: 4.4rem;
    border: 1px solid #cbd5e1;
    border-radius: 0.35rem;
    padding: 0.2rem 0.3rem;
    font: inherit;
  }

  .timestamps {
    margin: 0;
    color: #64748b;
    font-size: 0.78rem;
  }

  .compare {
    border: 1px solid #cbd5e1;
    border-radius: 0.6rem;
    padding: 0.6rem;
    background: #f8fafc;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .compare h4,
  .compare h5 {
    margin: 0;
  }

  .compare-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.75rem;
  }

  .compare-grid article {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
</style>
