<script lang="ts">
  import { createEventDispatcher, onMount } from 'svelte';
  import { api } from '$lib/api';
  import CitationPicker from '$lib/components/CitationPicker.svelte';
  import type { CitationDraft, EventDraft } from '$lib/components/formTypes';

  type PersonOption = {
    id: string;
    display_name: string;
  };

  export let mode: 'create' | 'edit' = 'create';
  export let initial: EventDraft | null = null;

  const dispatch = createEventDispatcher<{ saved: { id: string }; cancel: void; error: string }>();

  let saving = false;
  let formError = '';
  let loadingPeople = false;
  let people: PersonOption[] = [];

  let draft: EventDraft = {
    id: initial?.id,
    eventType: initial?.eventType ?? 'Birth',
    date: initial?.date ?? '',
    placeId: initial?.placeId ?? '',
    description: initial?.description ?? '',
    participants: initial?.participants.length
      ? initial.participants.map((participant) => ({ ...participant }))
      : [{ personId: '', role: 'Principal' }],
    citations: initial?.citations ?? []
  };

  async function attachCitations(eventId: string, citations: CitationDraft[]): Promise<void> {
    if (citations.length === 0) {
      return;
    }

    const assertions = await api.get<Record<string, Array<{ assertion_id: string }>>>(
      `/api/v1/events/${eventId}/assertions`
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

  const eventTypes = [
    'Birth',
    'Death',
    'Marriage',
    'Census',
    'Baptism',
    'Burial',
    'Migration',
    'Occupation',
    'Residence',
    'Immigration',
    'Emigration',
    'Naturalization',
    'Probate',
    'Will',
    'Graduation',
    'Retirement'
  ];

  const roles = [
    'Principal',
    'Witness',
    'Godparent',
    'Informant',
    'Clergy',
    'Registrar',
    'Celebrant',
    'Parent',
    'Spouse',
    'Child',
    'Servant',
    'Boarder'
  ];

  async function loadPeople(): Promise<void> {
    loadingPeople = true;
    try {
      people = await api.get<PersonOption[]>('/api/v1/persons?limit=200&offset=0');
    } catch (error) {
      formError = error instanceof Error ? error.message : 'Failed to load people';
    } finally {
      loadingPeople = false;
    }
  }

  function addParticipant(): void {
    draft.participants = [...draft.participants, { personId: '', role: 'Principal' }];
  }

  function removeParticipant(index: number): void {
    if (draft.participants.length <= 1) {
      return;
    }

    draft.participants = draft.participants.filter((_, i) => i !== index);
  }

  async function submit(): Promise<void> {
    formError = '';

    if (!draft.eventType.trim()) {
      formError = 'Event type is required.';
      return;
    }

    const participants = draft.participants.filter((participant) => participant.personId);

    const payload = {
      event_type: draft.eventType,
      date: draft.date || null,
      place_id: draft.placeId || null,
      description: draft.description || null
    };

    saving = true;

    try {
      let eventId = draft.id;

      if (mode === 'edit' && eventId) {
        await api.put(`/api/v1/events/${eventId}`, payload);
      } else {
        const created = await api.post<{ id: string }>('/api/v1/events', payload);
        eventId = created.id;
      }

      if (!eventId) {
        throw new Error('Event ID missing after save.');
      }

      for (const participant of participants) {
        await api.post(`/api/v1/events/${eventId}/participants`, {
          person_id: participant.personId,
          role: participant.role
        });
      }

      await attachCitations(eventId, draft.citations);

      dispatch('saved', { id: eventId });
    } catch (error) {
      formError = error instanceof Error ? error.message : 'Failed to save event';
      dispatch('error', formError);
    } finally {
      saving = false;
    }
  }

  function cancel(): void {
    dispatch('cancel');
  }

  onMount(async () => {
    await loadPeople();
  });
</script>

<div class="form">
  <h2>{mode === 'create' ? 'New event' : 'Edit event'}</h2>

  {#if loadingPeople}
    <p>Loading persons…</p>
  {/if}

  <section class="grid-two">
    <label>
      Event type
      <select bind:value={draft.eventType}>
        {#each eventTypes as eventType}
          <option>{eventType}</option>
        {/each}
      </select>
    </label>

    <label>
      Date
      <input bind:value={draft.date} placeholder="YYYY / YYYY-MM / YYYY-MM-DD / ABT 1880" />
    </label>

    <label>
      Place ID
      <input bind:value={draft.placeId} placeholder="Optional place UUID" />
    </label>

    <label>
      Description
      <input bind:value={draft.description} placeholder="Optional" />
    </label>
  </section>

  <section>
    <h3>Participants</h3>
    {#each draft.participants as participant, i}
      <div class="row">
        <select bind:value={draft.participants[i].personId}>
          <option value="">Select person</option>
          {#each people as person}
            <option value={person.id}>{person.display_name}</option>
          {/each}
        </select>

        <select bind:value={draft.participants[i].role}>
          {#each roles as role}
            <option>{role}</option>
          {/each}
        </select>

        <button type="button" on:click={() => removeParticipant(i)}>−</button>
      </div>
    {/each}

    <button type="button" on:click={addParticipant}>+ Add participant</button>
  </section>

  <CitationPicker bind:value={draft.citations} />

  {#if formError}
    <p class="error">{formError}</p>
  {/if}

  <div class="actions">
    <button type="button" class="secondary" on:click={cancel} disabled={saving}>Cancel</button>
    <button type="button" on:click={submit} disabled={saving}>{saving ? 'Saving…' : 'Save event'}</button>
  </div>
</div>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
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
  select {
    border: 1px solid #cbd5e1;
    border-radius: 0.45rem;
    padding: 0.45rem 0.55rem;
    font: inherit;
  }

  .row {
    display: grid;
    grid-template-columns: 2fr 1fr auto;
    gap: 0.5rem;
    margin-bottom: 0.5rem;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }

  button {
    border: 0;
    border-radius: 0.45rem;
    background: #2563eb;
    color: #fff;
    padding: 0.45rem 0.7rem;
    cursor: pointer;
  }

  .secondary {
    background: #64748b;
  }

  .error {
    margin: 0;
    color: #b91c1c;
    font-size: 0.9rem;
  }
</style>
