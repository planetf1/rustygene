<script lang="ts">
  import { createEventDispatcher, onMount } from 'svelte';
  import { api } from '$lib/api';

  type PartnerLink = 'Married' | 'Unmarried' | 'Unknown';

  type PersonOption = {
    id: string;
    display_name: string;
  };

  export type FamilyDraft = {
    id?: string;
    partner1Id: string;
    partner2Id: string;
    childIds: string[];
    partnerLink: PartnerLink;
    marriageDate: string;
    marriagePlace: string;
    notes: string;
  };

  export let mode: 'create' | 'edit' = 'create';
  export let initial: FamilyDraft | null = null;

  const dispatch = createEventDispatcher<{ saved: { id: string }; cancel: void; error: string }>();

  let saving = false;
  let loadingPeople = false;
  let formError = '';
  let people: PersonOption[] = [];

  let draft: FamilyDraft = {
    id: initial?.id,
    partner1Id: initial?.partner1Id ?? '',
    partner2Id: initial?.partner2Id ?? '',
    childIds: initial?.childIds ? [...initial.childIds] : [],
    partnerLink: initial?.partnerLink ?? 'Unknown',
    marriageDate: initial?.marriageDate ?? '',
    marriagePlace: initial?.marriagePlace ?? '',
    notes: initial?.notes ?? ''
  };

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

  function toggleChild(personId: string): void {
    if (draft.childIds.includes(personId)) {
      draft.childIds = draft.childIds.filter((id) => id !== personId);
      return;
    }

    draft.childIds = [...draft.childIds, personId];
  }

  async function submit(): Promise<void> {
    formError = '';

    if (!draft.partner1Id && !draft.partner2Id && draft.childIds.length === 0) {
      formError = 'Pick at least one partner or one child.';
      return;
    }

    const payload = {
      partner1_id: draft.partner1Id || null,
      partner2_id: draft.partner2Id || null,
      partner_link: draft.partnerLink,
      child_ids: draft.childIds
    };

    saving = true;

    try {
      if (mode === 'edit' && draft.id) {
        await api.put(`/api/v1/families/${draft.id}`, payload);
        dispatch('saved', { id: draft.id });
      } else {
        const created = await api.post<{ id: string }>('/api/v1/families', payload);
        dispatch('saved', { id: created.id });
      }
    } catch (error) {
      formError = error instanceof Error ? error.message : 'Failed to save family';
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
  <h2>{mode === 'create' ? 'New family' : 'Edit family'}</h2>

  {#if loadingPeople}
    <p>Loading persons…</p>
  {/if}

  <section class="grid-two">
    <label>
      Partner 1
      <select bind:value={draft.partner1Id}>
        <option value="">None</option>
        {#each people as person}
          <option value={person.id}>{person.display_name}</option>
        {/each}
      </select>
    </label>

    <label>
      Partner 2
      <select bind:value={draft.partner2Id}>
        <option value="">None</option>
        {#each people as person}
          <option value={person.id}>{person.display_name}</option>
        {/each}
      </select>
    </label>

    <label>
      Partner link
      <select bind:value={draft.partnerLink}>
        <option>Married</option>
        <option>Unmarried</option>
        <option>Unknown</option>
      </select>
    </label>

    <label>
      Marriage date
      <input bind:value={draft.marriageDate} placeholder="YYYY / YYYY-MM / YYYY-MM-DD / ABT 1880" />
    </label>

    <label>
      Marriage place
      <input bind:value={draft.marriagePlace} placeholder="Optional" />
    </label>
  </section>

  <section>
    <h3>Children</h3>
    <div class="child-grid">
      {#each people as person}
        <label class="checkbox-row">
          <input
            type="checkbox"
            checked={draft.childIds.includes(person.id)}
            on:change={() => toggleChild(person.id)}
          />
          <span>{person.display_name}</span>
        </label>
      {/each}
    </div>
  </section>

  <label>
    Notes
    <textarea bind:value={draft.notes} rows="3" placeholder="Free text notes"></textarea>
  </label>

  {#if formError}
    <p class="error">{formError}</p>
  {/if}

  <div class="actions">
    <button type="button" class="secondary" on:click={cancel} disabled={saving}>Cancel</button>
    <button type="button" on:click={submit} disabled={saving}>{saving ? 'Saving…' : 'Save family'}</button>
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
  select,
  textarea {
    border: 1px solid #cbd5e1;
    border-radius: 0.45rem;
    padding: 0.45rem 0.55rem;
    font: inherit;
  }

  .child-grid {
    max-height: 12rem;
    overflow: auto;
    border: 1px solid #e2e8f0;
    border-radius: 0.45rem;
    padding: 0.5rem;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.3rem 0.8rem;
  }

  .checkbox-row {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 0.4rem;
  }

  .checkbox-row input {
    width: auto;
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
