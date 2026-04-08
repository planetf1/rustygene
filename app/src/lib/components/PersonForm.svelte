<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { api } from '$lib/api';
  import CitationPicker from '$lib/components/CitationPicker.svelte';
  import type { CitationDraft, PersonDraft } from '$lib/components/formTypes';

  export let mode: 'create' | 'edit' = 'create';
  export let initial: PersonDraft | null = null;

  const dispatch = createEventDispatcher<{ saved: { id: string }; cancel: void; error: string }>();

  let saving = false;
  let formError = '';

  let draft: PersonDraft = {
    id: initial?.id,
    givenNames: initial?.givenNames.length ? [...initial.givenNames] : [''],
    surnames:
      initial?.surnames.length
        ? initial.surnames.map((surname) => ({ ...surname }))
        : [{ value: '', originType: 'Unknown', connector: '' }],
    nameType: initial?.nameType ?? 'Birth',
    sortAs: initial?.sortAs ?? '',
    callName: initial?.callName ?? '',
    gender: initial?.gender ?? 'Unknown',
    birthDate: initial?.birthDate ?? '',
    birthPlace: initial?.birthPlace ?? '',
    deathDate: initial?.deathDate ?? '',
    deathPlace: initial?.deathPlace ?? '',
    notes: initial?.notes ?? '',
    citations: initial?.citations ?? []
  };

  async function attachCitations(personId: string, citations: CitationDraft[]): Promise<void> {
    if (citations.length === 0) {
      return;
    }

    const assertions = await api.get<Record<string, Array<{ assertion_id: string }>>>(
      `/api/v1/persons/${personId}/assertions`
    );
    const targetAssertionId = Object.values(assertions).flatMap((rows) => rows).find((row) => row.assertion_id)
      ?.assertion_id;

    if (!targetAssertionId) {
      return;
    }

    for (const citation of citations) {
      await api.post('/api/v1/citations', {
        source_id: citation.sourceId,
        assertion_id: targetAssertionId,
        citation_note: citation.citationNote || null,
        volume: citation.volume || null,
        page: citation.page || null,
        folio: citation.folio || null,
        entry: citation.entry || null,
        confidence_level: citation.confidenceLevel,
        transcription: citation.transcription || null
      });
    }
  }

  function addGivenName(): void {
    draft.givenNames = [...draft.givenNames, ''];
  }

  function removeGivenName(index: number): void {
    if (draft.givenNames.length <= 1) {
      return;
    }

    draft.givenNames = draft.givenNames.filter((_, i) => i !== index);
  }

  function addSurname(): void {
    draft.surnames = [
      ...draft.surnames,
      {
        value: '',
        originType: 'Unknown',
        connector: ''
      }
    ];
  }

  function removeSurname(index: number): void {
    if (draft.surnames.length <= 1) {
      return;
    }

    draft.surnames = draft.surnames.filter((_, i) => i !== index);
  }

  async function submit(): Promise<void> {
    formError = '';

    const cleanGiven = draft.givenNames.map((name) => name.trim()).filter((name) => name.length > 0);
    const cleanSurnames = draft.surnames
      .map((surname) => ({
        value: surname.value.trim(),
        origin_type: surname.originType,
        connector: surname.connector.trim() || null
      }))
      .filter((surname) => surname.value.length > 0);

    if (cleanGiven.length === 0 && cleanSurnames.length === 0) {
      formError = 'At least one given name or surname is required.';
      return;
    }

    const payload = {
      given_names: cleanGiven.length ? cleanGiven : ['Unknown'],
      surnames: cleanSurnames.length ? cleanSurnames : [{ value: 'Unknown', origin_type: 'Unknown', connector: null }],
      name_type: draft.nameType,
      birth_date: draft.birthDate
        ? {
            Textual: {
              text: draft.birthDate
            }
          }
        : null,
      birth_place: draft.birthPlace || null,
      gender: draft.gender,
      call_name: draft.callName || null,
      sort_as: draft.sortAs || null,
      prefix: null,
      suffix: null
    };

    saving = true;

    try {
      const path = mode === 'edit' && draft.id ? `/api/v1/persons/${draft.id}` : '/api/v1/persons';
      const method = mode === 'edit' ? api.put<{ id: string }> : api.post<{ id: string }>;
      const response = await method(path, payload);
      const personId = response.id;

      await attachCitations(personId, draft.citations);
      dispatch('saved', { id: personId });
    } catch (error) {
      formError = error instanceof Error ? error.message : 'Failed to save person';
      dispatch('error', formError);
    } finally {
      saving = false;
    }
  }

  function cancel(): void {
    dispatch('cancel');
  }
</script>

<div class="form">
  <h2>{mode === 'create' ? 'New person' : 'Edit person'}</h2>

  <section>
    <h3>Given names</h3>
    {#each draft.givenNames as given, i}
      <div class="row">
        <input bind:value={draft.givenNames[i]} placeholder="Given name" />
        <button type="button" class="btn-secondary" on:click={() => removeGivenName(i)}>−</button>
      </div>
    {/each}
    <button type="button" class="btn-secondary" on:click={addGivenName}>+ Add given name</button>
  </section>

  <section>
    <h3>Surnames</h3>
    {#each draft.surnames as surname, i}
      <div class="stacked-row">
        <input bind:value={draft.surnames[i].value} placeholder="Surname" />
        <select bind:value={draft.surnames[i].originType}>
          <option>Patronymic</option>
          <option>Matronymic</option>
          <option>Toponymic</option>
          <option>Occupational</option>
          <option>Unknown</option>
        </select>
        <input bind:value={draft.surnames[i].connector} placeholder="Connector (optional)" />
        <button type="button" class="btn-secondary" on:click={() => removeSurname(i)}>−</button>
      </div>
    {/each}
    <button type="button" class="btn-secondary" on:click={addSurname}>+ Add surname</button>
  </section>

  <section class="grid-two">
    <label>
      Name type
      <select bind:value={draft.nameType}>
        <option>Birth</option>
        <option>Married</option>
        <option>Aka</option>
        <option>Nickname</option>
        <option>Other</option>
      </select>
    </label>

    <label>
      Gender
      <select bind:value={draft.gender}>
        <option>Male</option>
        <option>Female</option>
        <option>Other</option>
        <option>Unknown</option>
      </select>
    </label>

    <label>
      Sort as
      <input bind:value={draft.sortAs} placeholder="Optional" />
    </label>

    <label>
      Call name
      <input bind:value={draft.callName} placeholder="Optional" />
    </label>

    <label>
      Birth date
      <input bind:value={draft.birthDate} placeholder="YYYY / YYYY-MM / YYYY-MM-DD / ABT 1880" />
    </label>

    <label>
      Birth place
      <input bind:value={draft.birthPlace} placeholder="Optional" />
    </label>

    <label>
      Death date
      <input bind:value={draft.deathDate} placeholder="YYYY / YYYY-MM / YYYY-MM-DD / ABT 1880" />
    </label>

    <label>
      Death place
      <input bind:value={draft.deathPlace} placeholder="Optional" />
    </label>
  </section>

  <label>
    Notes
    <textarea bind:value={draft.notes} rows="3" placeholder="Notes (free text)"></textarea>
  </label>

  <CitationPicker bind:value={draft.citations} />

  {#if formError}
    <p class="error">{formError}</p>
  {/if}

  <div class="actions">
    <button type="button" class="btn-secondary" on:click={cancel} disabled={saving}>Cancel</button>
    <button type="button" class="btn-primary" on:click={submit} disabled={saving}>{saving ? 'Saving…' : 'Save person'}</button>
  </div>
</div>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
  }

  h2,
  h3 {
    margin: 0;
  }

  section {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .row {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 0.5rem;
  }

  .stacked-row {
    display: grid;
    grid-template-columns: 2fr 1fr 1fr auto;
    gap: 0.5rem;
  }

  .grid-two {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.6rem;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    font-size: 0.92rem;
  }

  input,
  select,
  textarea {
    border: 1px solid var(--color-border);
    border-radius: 0.45rem;
    padding: 0.45rem 0.55rem;
    background: var(--color-surface);
    color: var(--color-text);
    font: inherit;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }



  .error {
    margin: 0;
    color: var(--color-danger);
    font-size: 0.9rem;
  }
</style>
