<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { addRecentItem } from '$lib/state.svelte';
  import { api } from '$lib/api';
  import FamilyForm from '$lib/components/FamilyForm.svelte';
  import type { FamilyDraft } from '$lib/components/formTypes';
  import AssertionList from '$lib/components/AssertionList.svelte';

  type FamilyDetail = {
    id: string;
    partner1?: { id: string; display_name: string };
    partner2?: { id: string; display_name: string };
    partner_link: 'Married' | 'Unmarried' | 'Unknown';
    children: { id: string; display_name: string; lineage_type: string }[];
    events: { id: string; event_type: string; date: string | null }[];
    assertion_counts: Record<string, number>;
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

  let id = '';
  $: id = $page.params.id ?? '';

  let detail: FamilyDetail | null = null;
  let assertionGroup: AssertionGroup = {};
  let people: PersonOption[] = [];
  let loading = false;
  let error = '';
  let showEdit = false;
  let selectedPartner = '';
  let selectedChild = '';

  function familyTitle(): string {
    const p1 = detail?.partner1?.display_name ?? 'Unknown';
    const p2 = detail?.partner2?.display_name ?? 'Unknown';
    return `Family of ${p1} and ${p2}`;
  }

  function toDraft(): FamilyDraft | null {
    if (!detail) {
      return null;
    }

    return {
      id,
      partner1Id: detail.partner1?.id ?? '',
      partner2Id: detail.partner2?.id ?? '',
      childIds: detail.children.map((child) => child.id),
      partnerLink: detail.partner_link,
      marriageDate: '',
      marriagePlace: '',
      notes: ''
    };
  }

  async function loadFamily(): Promise<void> {
    loading = true;
    error = '';

    try {
      const [family, assertions, personRows] = await Promise.all([
        api.get<FamilyDetail>(`/api/v1/families/${id}`),
        api.get<AssertionGroup>(`/api/v1/families/${id}/assertions`),
        api.get<PersonOption[]>('/api/v1/persons?limit=200&offset=0')
      ]);
      detail = family;
      assertionGroup = assertions;
      people = personRows;

      addRecentItem({
        entityType: 'family',
        id,
        displayName: familyTitle()
      });
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load family detail';
    } finally {
      loading = false;
    }
  }

  async function saveLinks(payload: { partner1_id: string | null; partner2_id: string | null; child_ids: string[] }): Promise<void> {
    if (!detail) {
      return;
    }

    await api.put(`/api/v1/families/${id}`, {
      ...payload,
      partner_link: detail.partner_link
    });
    await loadFamily();
  }

  async function addPartner(): Promise<void> {
    if (!detail || !selectedPartner) {
      return;
    }

    if (!detail.partner1) {
      await saveLinks({
        partner1_id: selectedPartner,
        partner2_id: detail.partner2?.id ?? null,
        child_ids: detail.children.map((child) => child.id)
      });
    } else {
      await saveLinks({
        partner1_id: detail.partner1.id,
        partner2_id: selectedPartner,
        child_ids: detail.children.map((child) => child.id)
      });
    }

    selectedPartner = '';
  }

  async function addChild(): Promise<void> {
    if (!detail || !selectedChild) {
      return;
    }

    const nextChildren = Array.from(new Set([...detail.children.map((child) => child.id), selectedChild]));
    await saveLinks({
      partner1_id: detail.partner1?.id ?? null,
      partner2_id: detail.partner2?.id ?? null,
      child_ids: nextChildren
    });
    selectedChild = '';
  }

  async function removeChild(childId: string): Promise<void> {
    if (!detail) {
      return;
    }

    const nextChildren = detail.children.map((child) => child.id).filter((idValue) => idValue !== childId);
    if (nextChildren.length === 0) {
      error = 'Backend currently requires at least one child to persist child-links update.';
      return;
    }

    await saveLinks({
      partner1_id: detail.partner1?.id ?? null,
      partner2_id: detail.partner2?.id ?? null,
      child_ids: nextChildren
    });
  }

  async function deleteFamily(): Promise<void> {
    const confirmed = confirm('Delete this family? This cannot be undone.');
    if (!confirmed) {
      return;
    }

    try {
      await api.del(`/api/v1/families/${id}`);
      await goto('/families');
    } catch (err) {
      error = err instanceof Error ? err.message : 'Delete failed';
    }
  }

  onMount(async () => {
    await loadFamily();
  });
</script>

{#if loading}
  <p>Loading family detail…</p>
{:else if error}
  <main class="panel">
    <h1>Family detail</h1>
    <p class="error">{error}</p>
  </main>
{:else if detail}
  <main class="panel">
    <header class="header">
      <div>
        <h1>{familyTitle()}</h1>
        <p>ID: <code>{id}</code></p>
      </div>
      <div class="actions">
        <button type="button" on:click={() => (showEdit = true)}>Edit</button>
        <button type="button" class="danger" on:click={deleteFamily}>Delete</button>
      </div>
    </header>

    <section>
      <h2>Partners</h2>
      <ul class="list">
        {#if detail.partner1}
          <li><button type="button" class="linkish" on:click={() => goto(`/persons/${detail?.partner1?.id ?? ''}`)}>{detail?.partner1?.display_name}</button> (partner)</li>
        {/if}
        {#if detail.partner2}
          <li><button type="button" class="linkish" on:click={() => goto(`/persons/${detail?.partner2?.id ?? ''}`)}>{detail?.partner2?.display_name}</button> (partner)</li>
        {/if}
        {#if !detail.partner1 && !detail.partner2}
          <li>No partners linked.</li>
        {/if}
      </ul>

      <div class="inline-actions">
        <select bind:value={selectedPartner}>
          <option value="">Select person to add partner</option>
          {#each people as person}
            <option value={person.id}>{person.display_name}</option>
          {/each}
        </select>
        <button type="button" on:click={addPartner} disabled={!selectedPartner}>Add partner</button>
      </div>
    </section>

    <section>
      <h2>Children</h2>
      <ul class="list">
        {#if detail.children.length === 0}
          <li>No children linked.</li>
        {:else}
          {#each detail.children as child}
            <li>
              <button type="button" class="linkish" on:click={() => goto(`/persons/${child.id}`)}>{child.display_name}</button>
              <span class="muted">({child.lineage_type})</span>
              <button type="button" class="small danger" on:click={() => removeChild(child.id)}>Remove</button>
            </li>
          {/each}
        {/if}
      </ul>

      <div class="inline-actions">
        <select bind:value={selectedChild}>
          <option value="">Select person to add child</option>
          {#each people as person}
            <option value={person.id}>{person.display_name}</option>
          {/each}
        </select>
        <button type="button" on:click={addChild} disabled={!selectedChild}>Add child</button>
      </div>
    </section>

    <section>
      <h2>Events</h2>
      {#if detail.events.length === 0}
        <p>No family events linked yet.</p>
      {:else}
        <ul class="list">
          {#each detail.events as event}
            <li>
              <button type="button" class="linkish" on:click={() => goto(`/events/${event.id}`)}>
                {event.event_type} — {event.date ?? 'No date'}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <AssertionList entityId={id} entityType="families" assertions={assertionGroup} on:updated={loadFamily} />
    </section>

    <section>
      <h2>Source citations</h2>
      <p>Assertion distribution: {Object.entries(detail.assertion_counts).map(([k, v]) => `${k}:${v}`).join(', ') || 'none'}.</p>
    </section>
  </main>

  {#if showEdit}
    <button type="button" class="overlay" aria-label="Close family edit panel" on:click={() => (showEdit = false)}></button>
    <aside class="slideover">
      <FamilyForm
        mode="edit"
        initial={toDraft()}
        on:cancel={() => (showEdit = false)}
        on:saved={(event: CustomEvent<{ id: string }>) => {
          showEdit = false;
          void goto(`/families/${event.detail.id}`);
          void loadFamily();
        }}
      />
    </aside>
  {/if}
{/if}

<style>
  .panel {
    background: linear-gradient(180deg, #ffffff 0%, #fff9ff 100%);
    border: 1px solid var(--rg-border, #e8def8);
    border-radius: 1rem;
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
    color: #6b6192;
  }

  .actions {
    display: inline-flex;
    gap: 0.5rem;
  }

  section {
    border: 1px solid #efe6ff;
    border-radius: 0.85rem;
    padding: 0.85rem;
    background: #fffdff;
  }

  section h2 {
    margin-top: 0;
    color: #593ca8;
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
    color: #6b6192;
    margin-left: 0.35rem;
  }

  select {
    border: 1px solid #dfd2f8;
    border-radius: 0.7rem;
    padding: 0.5rem 0.62rem;
    font: inherit;
    min-width: 17rem;
  }

  button {
    background: #2563eb;
    color: #fff;
    border: 0;
    border-radius: 0.7rem;
    padding: 0.45rem 0.72rem;
    cursor: pointer;
  }

  .small {
    padding: 0.2rem 0.45rem;
    font-size: 0.8rem;
    margin-left: 0.4rem;
  }

  .danger {
    background: #d03165;
  }

  .linkish {
    border: 0;
    background: transparent;
    color: #6a46dc;
    cursor: pointer;
    padding: 0;
    font: inherit;
    font-weight: 600;
  }

  .overlay {
    position: fixed;
    inset: 0;
    background: rgb(52 32 97 / 32%);
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
    border-left: 1px solid var(--rg-border, #e8def8);
    padding: 1rem;
    overflow: auto;
  }

  .error {
    color: #b91c1c;
    margin: 0;
  }

  code {
    background: #f5efff;
    padding: 0.1rem 0.3rem;
    border-radius: 0.35rem;
  }
</style>
