<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import {
    createCitationForAssertion,
    runSourceDrivenCreateEvent,
    runSourceDrivenCreatePerson,
    type CitationInput
  } from '$lib/sourceDrivenWorkflow';

  type SourceRow = {
    id: string;
    title: string;
    author?: string | null;
  };

  type PersonRow = {
    id: string;
    display_name: string;
  };

  type AssertionRow = {
    assertion_id: string;
    field: string;
    value: unknown;
    confidence: number;
  };

  type CitationContext = CitationInput & { sourceTitle: string };

  type SourceAction =
    | 'create-person'
    | 'create-event'
    | 'create-family'
    | 'append-person-assertion'
    | 'edit-person-assertion';

  let step = 1;
  let saving = false;
  let loading = false;
  let error = '';
  let success = '';

  let sources: SourceRow[] = [];
  let people: PersonRow[] = [];
  let sourceQuery = '';

  let selectedSourceId = '';
  let selectedSourceTitle = '';

  let newSourceTitle = '';
  let newSourceAuthor = '';
  let showCreateSource = false;

  let citation: CitationContext = {
    sourceId: '',
    sourceTitle: '',
    page: '',
    folio: '',
    entry: '',
    citationNote: '',
    confidenceLevel: null,
    transcription: '',
    dateAccessed: ''
  };

  let targetAction: SourceAction = 'create-person';

  let personDraft = {
    givenNames: '',
    surnames: '',
    gender: 'Unknown'
  };

  let eventDraft = {
    eventType: 'Birth',
    description: '',
    personId: ''
  };

  let familyDraft = {
    partner1Id: '',
    partner2Id: ''
  };

  let appendDraft = {
    personId: '',
    field: '',
    value: ''
  };

  let editDraft = {
    personId: '',
    assertionId: '',
    confidence: 0.8,
    preferred: false,
    status: 'proposed'
  };

  let personAssertions: AssertionRow[] = [];

  const eventTypes = ['Birth', 'Death', 'Marriage', 'Census', 'Baptism', 'Burial', 'Residence'];

  async function loadSourcesAndPeople(): Promise<void> {
    loading = true;
    error = '';
    try {
      const [sourceRows, personRows] = await Promise.all([
        api.get<SourceRow[]>('/api/v1/sources?limit=500&offset=0'),
        api.get<PersonRow[]>('/api/v1/persons?limit=500&offset=0')
      ]);
      sources = sourceRows;
      people = personRows;
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load source-driven entry data.';
    } finally {
      loading = false;
    }
  }

  function filteredSources(): SourceRow[] {
    const query = sourceQuery.trim().toLowerCase();
    if (!query) {
      return sources.slice(0, 30);
    }

    return sources
      .filter((source) => `${source.title} ${source.author ?? ''}`.toLowerCase().includes(query))
      .slice(0, 30);
  }

  async function createSourceInline(): Promise<void> {
    const title = newSourceTitle.trim();
    if (!title) {
      error = 'Source title is required.';
      return;
    }

    try {
      const created = await api.post<{ id: string }>('/api/v1/sources', {
        title,
        author: newSourceAuthor.trim() || null,
        publication_info: null,
        abbreviation: null,
        repository_refs: []
      });

      selectedSourceId = created.id;
      newSourceTitle = '';
      newSourceAuthor = '';
      showCreateSource = false;
      await loadSourcesAndPeople();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to create source.';
    }
  }

  function ensureStep(stepNumber: number): void {
    error = '';
    success = '';

    if (stepNumber === 2 && !selectedSourceId) {
      error = 'Choose a source before continuing.';
      return;
    }

    if (stepNumber === 3 && !selectedSourceId) {
      error = 'Choose a source before continuing.';
      return;
    }

    step = stepNumber;
  }

  async function createFamilyFromSource(): Promise<string> {
    const created = await api.post<{ id: string }>('/api/v1/families', {
      partner1_id: familyDraft.partner1Id || null,
      partner2_id: familyDraft.partner2Id || null,
      partner_link: 'Unknown',
      child_ids: []
    });

    return created.id;
  }

  async function appendAssertionToPerson(): Promise<string> {
    if (!appendDraft.personId) {
      throw new Error('Select a person for assertion append.');
    }
    if (!appendDraft.field.trim()) {
      throw new Error('Assertion field is required.');
    }

    let value: unknown = appendDraft.value;
    try {
      value = JSON.parse(appendDraft.value);
    } catch {
      value = appendDraft.value;
    }

    const created = await api.post<{ assertion_id: string }>(
      `/api/v1/persons/${appendDraft.personId}/assertions`,
      {
        field: appendDraft.field.trim(),
        value,
        confidence: 0.8,
        status: 'proposed',
        source_citations: []
      }
    );

    await createCitationForAssertion(api, created.assertion_id, citation);
    return appendDraft.personId;
  }

  async function updateExistingAssertionPreservingProvenance(): Promise<string> {
    if (!editDraft.personId || !editDraft.assertionId) {
      throw new Error('Select a person and assertion to edit.');
    }

    await api.put(`/api/v1/persons/${editDraft.personId}/assertions/${editDraft.assertionId}`, {
      confidence: editDraft.confidence,
      preferred: editDraft.preferred,
      status: editDraft.status
    });

    await createCitationForAssertion(api, editDraft.assertionId, citation);
    return editDraft.personId;
  }

  async function loadAssertionsForEdit(personId: string): Promise<void> {
    if (!personId) {
      personAssertions = [];
      editDraft.assertionId = '';
      return;
    }

    const grouped = await api.get<Record<string, AssertionRow[]>>(`/api/v1/persons/${personId}/assertions`);
    personAssertions = Object.values(grouped).flat();
    editDraft.assertionId = personAssertions[0]?.assertion_id ?? '';
  }

  async function submit(): Promise<void> {
    if (!citation.sourceId) {
      error = 'Select a source before saving.';
      return;
    }

    saving = true;
    error = '';
    success = '';

    try {
      let entityId = '';

      if (targetAction === 'create-person') {
        entityId = await runSourceDrivenCreatePerson(api, citation, {
          givenNames: personDraft.givenNames,
          surnames: personDraft.surnames,
          gender: personDraft.gender
        });
      } else if (targetAction === 'create-event') {
        entityId = await runSourceDrivenCreateEvent(api, citation, {
          eventType: eventDraft.eventType,
          description: eventDraft.description,
          personId: eventDraft.personId
        });
      } else if (targetAction === 'create-family') {
        entityId = await createFamilyFromSource();
      } else if (targetAction === 'append-person-assertion') {
        entityId = await appendAssertionToPerson();
      } else {
        entityId = await updateExistingAssertionPreservingProvenance();
      }

      success = `Saved source-driven workflow to ${targetAction} (${entityId}).`;
    } catch (err) {
      error = err instanceof Error ? err.message : 'Source-driven save failed.';
    } finally {
      saving = false;
    }
  }

  function onKeyboard(event: KeyboardEvent): void {
    if (event.altKey) {
      if (event.key === '1') ensureStep(1);
      if (event.key === '2') ensureStep(2);
      if (event.key === '3') ensureStep(3);
      if (event.key === '4') ensureStep(4);
    }

    if (event.ctrlKey && event.key === 'Enter' && step === 4 && !saving) {
      event.preventDefault();
      void submit();
    }
  }

  $: {
    const selected = sources.find((source) => source.id === selectedSourceId);
    selectedSourceTitle = selected?.title ?? '';
    citation.sourceId = selectedSourceId;
    citation.sourceTitle = selectedSourceTitle;
  }

  $: if (targetAction === 'edit-person-assertion') {
    void loadAssertionsForEdit(editDraft.personId);
  }

  onMount(async () => {
    await loadSourcesAndPeople();
  });
</script>

<svelte:window on:keydown={onKeyboard} />

<main class="panel">
  <header>
    <h1>Source-driven entry</h1>
    <p class="hint">Keyboard: Alt+1..4 moves steps, Ctrl+Enter saves on confirmation.</p>
  </header>

  {#if citation.sourceId}
    <section class="provenance-banner">
      <strong>Provenance mode active:</strong>
      <span>{citation.sourceTitle || citation.sourceId}</span>
      <span>{citation.page ? `Page ${citation.page}` : 'No page'}</span>
      <span>{citation.folio ? `Folio ${citation.folio}` : 'No folio'}</span>
    </section>
  {/if}

  {#if loading}
    <p>Loading source-driven data…</p>
  {/if}

  <nav class="steps" aria-label="Source-driven workflow steps">
    <button type="button" class:active={step === 1} on:click={() => ensureStep(1)}>1 Source</button>
    <button type="button" class:active={step === 2} on:click={() => ensureStep(2)}>2 Citation</button>
    <button type="button" class:active={step === 3} on:click={() => ensureStep(3)}>3 Target action</button>
    <button type="button" class:active={step === 4} on:click={() => ensureStep(4)}>4 Confirm</button>
  </nav>

  {#if step === 1}
    <section class="card">
      <h2>Step 1 · Choose source</h2>
      <label>
        Search
        <input bind:value={sourceQuery} placeholder="Type title or author" />
      </label>

      <label>
        Source
        <select bind:value={selectedSourceId}>
          <option value="">Select source</option>
          {#each filteredSources() as source}
            <option value={source.id}>{source.title}</option>
          {/each}
        </select>
      </label>

      <button type="button" class="secondary" on:click={() => (showCreateSource = !showCreateSource)}>
        {showCreateSource ? 'Cancel inline source' : 'Create source inline'}
      </button>

      {#if showCreateSource}
        <div class="inline-grid">
          <input bind:value={newSourceTitle} placeholder="Source title" />
          <input bind:value={newSourceAuthor} placeholder="Author" />
          <button type="button" on:click={createSourceInline}>Create source</button>
        </div>
      {/if}

      <div class="actions">
        <button type="button" on:click={() => ensureStep(2)}>Continue to citation</button>
      </div>
    </section>
  {/if}

  {#if step === 2}
    <section class="card">
      <h2>Step 2 · Citation details</h2>
      <div class="grid">
        <label>
          Page
          <input bind:value={citation.page} placeholder="Page" />
        </label>
        <label>
          Folio
          <input bind:value={citation.folio} placeholder="Folio" />
        </label>
        <label>
          Entry
          <input bind:value={citation.entry} placeholder="Entry" />
        </label>
        <label>
          Date accessed
          <input bind:value={citation.dateAccessed} placeholder="2026-04-02" />
        </label>
        <label>
          Confidence (0-3)
          <input
            type="number"
            min="0"
            max="3"
            value={citation.confidenceLevel ?? ''}
            on:input={(event) => {
              const raw = (event.currentTarget as HTMLInputElement).value;
              citation.confidenceLevel = raw === '' ? null : Number(raw);
            }}
          />
        </label>
      </div>

      <label>
        Citation note
        <input bind:value={citation.citationNote} placeholder="Optional citation note" />
      </label>

      <label>
        Transcription
        <textarea bind:value={citation.transcription} rows="3"></textarea>
      </label>

      <div class="actions">
        <button type="button" class="secondary" on:click={() => ensureStep(1)}>Back</button>
        <button type="button" on:click={() => ensureStep(3)}>Continue to target action</button>
      </div>
    </section>
  {/if}

  {#if step === 3}
    <section class="card">
      <h2>Step 3 · Target entity action</h2>
      <label>
        Action
        <select bind:value={targetAction}>
          <option value="create-person">Create person</option>
          <option value="create-event">Create event</option>
          <option value="create-family">Create family</option>
          <option value="append-person-assertion">Append person assertion</option>
          <option value="edit-person-assertion">Edit person assertion (preserve provenance)</option>
        </select>
      </label>

      {#if targetAction === 'create-person'}
        <div class="grid">
          <label>
            Given names (comma-separated)
            <input bind:value={personDraft.givenNames} placeholder="John, Quincy" />
          </label>
          <label>
            Surnames (comma-separated)
            <input bind:value={personDraft.surnames} placeholder="Adams" />
          </label>
          <label>
            Gender
            <select bind:value={personDraft.gender}>
              <option>Male</option>
              <option>Female</option>
              <option>Other</option>
              <option>Unknown</option>
            </select>
          </label>
        </div>
      {:else if targetAction === 'create-event'}
        <div class="grid">
          <label>
            Event type
            <select bind:value={eventDraft.eventType}>
              {#each eventTypes as eventType}
                <option value={eventType}>{eventType}</option>
              {/each}
            </select>
          </label>

          <label>
            Principal (optional)
            <select bind:value={eventDraft.personId}>
              <option value="">None</option>
              {#each people as person}
                <option value={person.id}>{person.display_name}</option>
              {/each}
            </select>
          </label>

          <label>
            Description
            <input bind:value={eventDraft.description} placeholder="Event statement" />
          </label>
        </div>
      {:else if targetAction === 'create-family'}
        <div class="grid">
          <label>
            Partner 1
            <select bind:value={familyDraft.partner1Id}>
              <option value="">None</option>
              {#each people as person}
                <option value={person.id}>{person.display_name}</option>
              {/each}
            </select>
          </label>

          <label>
            Partner 2
            <select bind:value={familyDraft.partner2Id}>
              <option value="">None</option>
              {#each people as person}
                <option value={person.id}>{person.display_name}</option>
              {/each}
            </select>
          </label>
        </div>
      {:else if targetAction === 'append-person-assertion'}
        <div class="grid">
          <label>
            Person
            <select bind:value={appendDraft.personId}>
              <option value="">Select person</option>
              {#each people as person}
                <option value={person.id}>{person.display_name}</option>
              {/each}
            </select>
          </label>

          <label>
            Field
            <input bind:value={appendDraft.field} placeholder="occupation" />
          </label>

          <label>
            Value (raw text or JSON)
            <input bind:value={appendDraft.value} placeholder="Carpenter" />
          </label>
        </div>
      {:else}
        <div class="grid">
          <label>
            Person
            <select
              bind:value={editDraft.personId}
              on:change={(event) => loadAssertionsForEdit((event.currentTarget as HTMLSelectElement).value)}
            >
              <option value="">Select person</option>
              {#each people as person}
                <option value={person.id}>{person.display_name}</option>
              {/each}
            </select>
          </label>

          <label>
            Assertion
            <select bind:value={editDraft.assertionId}>
              <option value="">Select assertion</option>
              {#each personAssertions as assertion}
                <option value={assertion.assertion_id}>{assertion.field}: {JSON.stringify(assertion.value)}</option>
              {/each}
            </select>
          </label>

          <label>
            Status
            <select bind:value={editDraft.status}>
              <option value="proposed">proposed</option>
              <option value="confirmed">confirmed</option>
              <option value="disputed">disputed</option>
              <option value="rejected">rejected</option>
            </select>
          </label>

          <label>
            Confidence (0.0-1.0)
            <input type="number" min="0" max="1" step="0.05" bind:value={editDraft.confidence} />
          </label>

          <label class="checkbox-row">
            <input type="checkbox" bind:checked={editDraft.preferred} />
            Preferred assertion
          </label>
        </div>
      {/if}

      <div class="actions">
        <button type="button" class="secondary" on:click={() => ensureStep(2)}>Back</button>
        <button type="button" on:click={() => ensureStep(4)}>Review summary</button>
      </div>
    </section>
  {/if}

  {#if step === 4}
    <section class="card">
      <h2>Step 4 · Confirmation summary</h2>
      <ul class="summary">
        <li><strong>Source:</strong> {citation.sourceTitle || citation.sourceId}</li>
        <li><strong>Citation:</strong> page={citation.page || '—'}, folio={citation.folio || '—'}, entry={citation.entry || '—'}</li>
        <li><strong>Action:</strong> {targetAction}</li>
      </ul>

      <p class="hint">Saving will link the selected citation to the created or edited assertion automatically.</p>

      <div class="actions">
        <button type="button" class="secondary" on:click={() => ensureStep(3)}>Back</button>
        <button type="button" disabled={saving} on:click={submit}>{saving ? 'Saving…' : 'Save source-driven entry'}</button>
      </div>
    </section>
  {/if}

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if success}
    <p class="success">{success}</p>
  {/if}
</main>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
    background: #ffffff;
    border: 1px solid #e2e8f0;
    border-radius: 0.75rem;
    padding: 1.25rem;
  }

  header h1 {
    margin: 0;
  }

  .steps {
    display: flex;
    gap: 0.45rem;
    flex-wrap: wrap;
  }

  .steps button {
    background: #cbd5e1;
    color: #0f172a;
  }

  .steps button.active {
    background: #2563eb;
    color: #ffffff;
  }

  .card {
    border: 1px solid #e2e8f0;
    border-radius: 0.65rem;
    padding: 0.8rem;
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
  }

  .provenance-banner {
    display: flex;
    gap: 0.55rem;
    flex-wrap: wrap;
    align-items: center;
    background: #ecfeff;
    border: 1px solid #67e8f9;
    border-radius: 0.5rem;
    padding: 0.45rem 0.55rem;
    color: #0e7490;
    font-size: 0.9rem;
  }

  .grid,
  .inline-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.6rem;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    font-size: 0.9rem;
  }

  .checkbox-row {
    flex-direction: row;
    align-items: center;
  }

  input,
  select,
  textarea,
  button {
    border: 1px solid #cbd5e1;
    border-radius: 0.45rem;
    padding: 0.45rem 0.55rem;
    font: inherit;
  }

  button {
    cursor: pointer;
    border: 0;
    background: #2563eb;
    color: #ffffff;
    width: fit-content;
  }

  .secondary {
    background: #64748b;
  }

  .actions {
    display: flex;
    gap: 0.5rem;
    justify-content: flex-end;
  }

  .summary {
    margin: 0;
    padding-left: 1.1rem;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .hint {
    margin: 0;
    color: #475569;
    font-size: 0.9rem;
  }

  .error {
    margin: 0;
    color: #b91c1c;
  }

  .success {
    margin: 0;
    color: #047857;
  }
</style>
