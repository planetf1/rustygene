<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { addRecentItem } from '$lib/state.svelte';
  import { api } from '$lib/api';
  import EventForm, { type EventDraft } from '$lib/components/EventForm.svelte';
  import AssertionList from '$lib/components/AssertionList.svelte';
  import NoteList from '$lib/components/NoteList.svelte';

  type EventDetail = {
    id: string;
    event_type: string;
    date: string | null;
    place_id: string | null;
    participants: { person_id: string; role: string }[];
    citations: { source_id?: string; citation_id?: string }[];
    confidence: number;
  };

  type PersonOption = {
    id: string;
    display_name: string;
  };

  type AssertionGroup = Record<
    string,
    {
      assertion_id: string;
      status: string;
      confidence: number;
      sources: { citation_id?: string; source_id?: string }[];
      value: unknown;
    }[]
  >;

  $: id = $page.params.id;

  let detail: EventDetail | null = null;
  let assertionGroup: AssertionGroup = {};
  let people: PersonOption[] = [];
  let loading = false;
  let error = '';
  let showEdit = false;

  let selectedPerson = '';
  let selectedRole = 'Principal';

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

  function participantName(personId: string): string {
    return people.find((person) => person.id === personId)?.display_name ?? personId;
  }

  function toDraft(): EventDraft | null {
    if (!detail) {
      return null;
    }

    return {
      id,
      eventType: detail.event_type,
      date: detail.date ?? '',
      placeId: detail.place_id ?? '',
      description: '',
      participants: detail.participants.map((participant) => ({
        personId: participant.person_id,
        role: participant.role
      })),
      citations: []
    };
  }

  async function loadEvent(): Promise<void> {
    loading = true;
    error = '';

    try {
      const [eventDetail, assertions, personRows] = await Promise.all([
        api.get<EventDetail>(`/api/v1/events/${id}`),
        api.get<AssertionGroup>(`/api/v1/events/${id}/assertions`),
        api.get<PersonOption[]>('/api/v1/persons?limit=200&offset=0')
      ]);
      detail = eventDetail;
      assertionGroup = assertions;
      people = personRows;

      addRecentItem({
        entityType: 'event',
        id,
        displayName: `${eventDetail.event_type} ${eventDetail.date ?? ''}`.trim()
      });
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load event detail';
    } finally {
      loading = false;
    }
  }

  async function addParticipant(): Promise<void> {
    if (!selectedPerson) {
      return;
    }

    try {
      await api.post(`/api/v1/events/${id}/participants`, {
        person_id: selectedPerson,
        role: selectedRole
      });
      selectedPerson = '';
      selectedRole = 'Principal';
      await loadEvent();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to add participant';
    }
  }

  async function removeParticipant(personId: string): Promise<void> {
    try {
      await api.del(`/api/v1/events/${id}/participants/${personId}`);
      await loadEvent();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to remove participant';
    }
  }

  async function deleteEvent(): Promise<void> {
    const confirmed = confirm('Delete this event? This cannot be undone.');
    if (!confirmed) {
      return;
    }

    try {
      await api.del(`/api/v1/events/${id}`);
      await goto('/events');
    } catch (err) {
      error = err instanceof Error ? err.message : 'Delete failed';
    }
  }

  onMount(async () => {
    await loadEvent();
  });
</script>

{#if loading}
  <p>Loading event detail…</p>
{:else if error}
  <main class="panel">
    <h1>Event detail</h1>
    <p class="error">{error}</p>
  </main>
{:else if detail}
  <main class="panel">
    <header class="header">
      <div>
        <h1>{detail.event_type}</h1>
        <p>{detail.date ?? 'No date'} · {detail.place_id ?? 'No place'}</p>
      </div>
      <div class="actions">
        <button type="button" on:click={() => (showEdit = true)}>Edit</button>
        <button type="button" class="danger" on:click={deleteEvent}>Delete</button>
      </div>
    </header>

    <section>
      <h2>Participants</h2>
      {#if detail.participants.length === 0}
        <p>No participants linked yet.</p>
      {:else}
        <ul class="list">
          {#each detail.participants as participant}
            <li>
              <button type="button" class="linkish" on:click={() => goto(`/persons/${participant.person_id}`)}>
                {participantName(participant.person_id)}
              </button>
              <span class="muted">({participant.role})</span>
              <button type="button" class="small danger" on:click={() => removeParticipant(participant.person_id)}>
                Remove
              </button>
            </li>
          {/each}
        </ul>
      {/if}

      <div class="inline-actions">
        <select bind:value={selectedPerson}>
          <option value="">Select participant</option>
          {#each people as person}
            <option value={person.id}>{person.display_name}</option>
          {/each}
        </select>
        <select bind:value={selectedRole}>
          {#each roles as role}
            <option>{role}</option>
          {/each}
        </select>
        <button type="button" disabled={!selectedPerson} on:click={addParticipant}>Add participant</button>
      </div>
    </section>

    <section>
      <AssertionList entityId={id} entityType="events" assertions={assertionGroup} on:updated={loadEvent} />
    </section>

    <section>
      <h2>Citations</h2>
      {#if detail.citations.length === 0}
        <p>No citations attached.</p>
      {:else}
        <ul class="list">
          {#each detail.citations as citation}
            <li><code>{citation.citation_id ?? citation.source_id ?? 'citation'}</code></li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <h2>Notes</h2>
      <NoteList entityId={id} entityType="event" />
    </section>

    <section>
      <h2>Media</h2>
      <p>Media endpoint currently returns 501. Placeholder ready.</p>
    </section>
  </main>

  {#if showEdit}
    <button type="button" class="overlay" aria-label="Close event edit panel" on:click={() => (showEdit = false)}></button>
    <aside class="slideover">
      <EventForm
        mode="edit"
        initial={toDraft()}
        on:cancel={() => (showEdit = false)}
        on:saved={(event: CustomEvent<{ id: string }>) => {
          showEdit = false;
          void goto(`/events/${event.detail.id}`);
          void loadEvent();
        }}
      />
    </aside>
  {/if}
{/if}

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

  .header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 1rem;
  }

  .header h1 {
    margin: 0;
  }

  .header p {
    margin: 0.2rem 0 0;
    color: #64748b;
  }

  .actions {
    display: inline-flex;
    gap: 0.5rem;
  }

  .list {
    margin: 0;
    padding-left: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .inline-actions {
    margin-top: 0.55rem;
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }

  .muted {
    color: #64748b;
    margin-left: 0.35rem;
  }

  select {
    border: 1px solid #cbd5e1;
    border-radius: 0.45rem;
    padding: 0.45rem 0.55rem;
    font: inherit;
    min-width: 14rem;
  }

  button {
    background: #2563eb;
    color: #fff;
    border: 0;
    border-radius: 0.45rem;
    padding: 0.45rem 0.7rem;
    cursor: pointer;
  }

  .small {
    padding: 0.2rem 0.45rem;
    font-size: 0.8rem;
    margin-left: 0.4rem;
  }

  .danger {
    background: #dc2626;
  }

  .linkish {
    border: 0;
    background: transparent;
    color: #1d4ed8;
    cursor: pointer;
    padding: 0;
    font: inherit;
  }

  .overlay {
    position: fixed;
    inset: 0;
    background: rgb(15 23 42 / 35%);
    border: 0;
    width: 100%;
    padding: 0;
    border-radius: 0;
  }

  .slideover {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(760px, 100%);
    background: #ffffff;
    border-left: 1px solid #e2e8f0;
    padding: 1rem;
    overflow: auto;
  }

  .error {
    color: #b91c1c;
    margin: 0;
  }

  code {
    background: #f1f5f9;
    padding: 0.1rem 0.3rem;
    border-radius: 0.25rem;
  }
</style>
