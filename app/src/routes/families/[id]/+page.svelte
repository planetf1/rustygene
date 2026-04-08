<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { addRecentItem } from '$lib/state.svelte';
  import { api } from '$lib/api';
  import FamilyForm from '$lib/components/FamilyForm.svelte';
  import type { FamilyDraft } from '$lib/components/formTypes';
  import AssertionList from '$lib/components/AssertionList.svelte';
  import BreadcrumbTrail from '$lib/components/BreadcrumbTrail.svelte';
  import EvidenceTracePanel from '$lib/components/EvidenceTracePanel.svelte';
  import RelatedRecordsGraph from '$lib/components/RelatedRecordsGraph.svelte';

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
  let backLabel = '';
  let backHref = '';
  let editingCore = false;
  let corePartnerLink: FamilyDetail['partner_link'] = 'Unknown';
  let coreConfidence = 0.9;
  let coreEvidenceType: 'direct' | 'indirect' | 'negative' = 'direct';
  let coreError = '';
  let savingCore = false;

  function familyTitle(): string {
    const p1 = detail?.partner1?.display_name ?? 'Unknown';
    const p2 = detail?.partner2?.display_name ?? 'Unknown';
    return `Family of ${p1} and ${p2}`;
  }

  function readBackContext(): void {
    const from = $page.url.searchParams.get('from') ?? '';
    const back = $page.url.searchParams.get('back') ?? '';
    backLabel = from ? `← Back to ${from}` : '← Back';
    backHref = back;
  }

  function originHref(): string {
    return `/families/${id}`;
  }

  function withNavContext(target: string): string {
    const current = `${$page.url.pathname}${$page.url.search}`;
    const sep = target.includes('?') ? '&' : '?';
    return `${target}${sep}from=${encodeURIComponent(familyTitle())}&back=${encodeURIComponent(current)}`;
  }

  function breadcrumbItems(): Array<{ label: string; href?: string }> {
    const items: Array<{ label: string; href?: string }> = [{ label: 'Families', href: '/families' }];
    if (backHref && backHref !== '/families') {
      items.push({ label: backLabel.replace('← Back to ', ''), href: backHref });
    }
    items.push({ label: familyTitle() });
    return items;
  }

  function relatedNodes(): Array<{ id: string; label: string; href: string; kind: 'person' | 'family' | 'event' | 'source' | 'citation' | 'repository' | 'media' | 'other' }> {
    if (!detail) {
      return [];
    }

    const partnerNodes = [detail.partner1, detail.partner2]
      .filter(Boolean)
      .map((person) => ({
        id: `person-${person?.id ?? ''}`,
        label: person?.display_name ?? 'Person',
        href: withNavContext(`/persons/${person?.id ?? ''}`),
        kind: 'person' as const
      }));

    const childNodes = detail.children.map((child) => ({
      id: `person-${child.id}`,
      label: child.display_name,
      href: withNavContext(`/persons/${child.id}`),
      kind: 'person' as const
    }));

    const eventNodes = detail.events.map((event) => ({
      id: `event-${event.id}`,
      label: event.event_type,
      href: withNavContext(`/events/${event.id}`),
      kind: 'event' as const
    }));

    return [...partnerNodes, ...childNodes, ...eventNodes].slice(0, 14);
  }

  function evidenceRefs(): Array<{ citation_id?: string; source_id?: string; label?: string }> {
    return Object.entries(assertionGroup).flatMap(([field, rows]) =>
      rows.flatMap((row) =>
        (row.sources ?? []).map((source) => ({
          citation_id: source.citation_id,
          source_id: source.source_id,
          label: `Assertion: ${field}`
        }))
      )
    );
  }

  function openCoreEdit(): void {
    if (!detail) {
      return;
    }

    corePartnerLink = detail.partner_link;
    coreConfidence = 0.9;
    coreEvidenceType = 'direct';
    coreError = '';
    editingCore = true;
  }

  function cancelCoreEdit(): void {
    editingCore = false;
    coreError = '';
  }

  async function saveCoreEdit(): Promise<void> {
    if (!detail) {
      return;
    }

    savingCore = true;
    coreError = '';

    try {
      await api.put(`/api/v1/families/${id}`, {
        partner1_id: detail.partner1?.id ?? null,
        partner2_id: detail.partner2?.id ?? null,
        child_ids: detail.children.map((child) => child.id),
        partner_link: corePartnerLink
      });

      await api.post(`/api/v1/families/${id}/assertions`, {
        field: 'partner_link',
        value: corePartnerLink,
        confidence: coreConfidence,
        evidence_type: coreEvidenceType,
        status: 'proposed'
      });

      editingCore = false;
      await loadFamily();
    } catch (err) {
      coreError = err instanceof Error ? err.message : 'Failed to save core family fields';
    } finally {
      savingCore = false;
    }
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
        api.get<{ total: number; items: PersonOption[] }>('/api/v1/persons?limit=200&offset=0')
      ]);
      detail = family;
      assertionGroup = assertions;
      people = personRows.items;

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
    readBackContext();
    await loadFamily();
  });
</script>

{#if loading}
  <p>Loading family profile…</p>
{:else if error}
  <main class="panel">
    <h1>🏡 Family profile</h1>
    <p class="error">{error}</p>
  </main>
{:else if detail}
  <main class="panel">
    <BreadcrumbTrail items={breadcrumbItems()} />

    {#if backHref}
      <button type="button" class="back-link" on:click={() => goto(backHref)}>{backLabel}</button>
    {/if}

    <header class="header">
      <div>
        <h1>{familyTitle()}</h1>
        <p>ID: <code>{id}</code></p>
      </div>
      <div class="actions">
        <button type="button" class="secondary" on:click={() => (editingCore ? cancelCoreEdit() : openCoreEdit())}>
          {editingCore ? 'Cancel quick edit' : 'Quick edit core fields'}
        </button>
        <button type="button" on:click={() => (showEdit = true)}>Edit family</button>
        <button type="button" class="danger" on:click={deleteFamily}>Remove family</button>
      </div>
    </header>

    {#if editingCore}
      <section class="core-edit">
        <h2>Inline core edit</h2>
        <div class="core-grid">
          <label>
            Partner link
            <select bind:value={corePartnerLink}>
              <option value="Married">Married</option>
              <option value="Unmarried">Unmarried</option>
              <option value="Unknown">Unknown</option>
            </select>
          </label>
          <label>
            Confidence
            <input type="number" min="0" max="1" step="0.01" bind:value={coreConfidence} />
          </label>
          <label>
            Evidence type
            <select bind:value={coreEvidenceType}>
              <option value="direct">Direct</option>
              <option value="indirect">Indirect</option>
              <option value="negative">Negative</option>
            </select>
          </label>
        </div>
        {#if coreError}
          <p class="error">{coreError}</p>
        {/if}
        <div class="inline-actions">
          <button type="button" on:click={saveCoreEdit} disabled={savingCore}>{savingCore ? 'Saving…' : 'Save'}</button>
          <button type="button" class="secondary" on:click={cancelCoreEdit}>Cancel</button>
        </div>
      </section>
    {/if}

    <section>
      <RelatedRecordsGraph centerLabel={familyTitle()} nodes={relatedNodes()} />
    </section>

    <section>
      <h2>💞 Partners</h2>
      <ul class="list">
        {#if detail.partner1}
          <li><button type="button" class="linkish" on:click={() => goto(withNavContext(`/persons/${detail?.partner1?.id ?? ''}`))}>{detail?.partner1?.display_name}</button> (partner)</li>
        {/if}
        {#if detail.partner2}
          <li><button type="button" class="linkish" on:click={() => goto(withNavContext(`/persons/${detail?.partner2?.id ?? ''}`))}>{detail?.partner2?.display_name}</button> (partner)</li>
        {/if}
        {#if !detail.partner1 && !detail.partner2}
          <li>No partners linked yet.</li>
        {/if}
      </ul>

      <div class="inline-actions">
        <select bind:value={selectedPartner}>
          <option value="">Choose person to add as partner</option>
          {#each people as person}
            <option value={person.id}>{person.display_name}</option>
          {/each}
        </select>
        <button type="button" on:click={addPartner} disabled={!selectedPartner}>Add partner 💛</button>
      </div>
    </section>

    <section>
      <h2>🧒 Children</h2>
      <ul class="list">
        {#if detail.children.length === 0}
          <li>No children linked yet.</li>
        {:else}
          {#each detail.children as child}
            <li>
              <button type="button" class="linkish" on:click={() => goto(withNavContext(`/persons/${child.id}`))}>{child.display_name}</button>
              <span class="muted">({child.lineage_type})</span>
              <button type="button" class="small danger" on:click={() => removeChild(child.id)}>Remove link</button>
            </li>
          {/each}
        {/if}
      </ul>

      <div class="inline-actions">
        <select bind:value={selectedChild}>
          <option value="">Choose person to add as child</option>
          {#each people as person}
            <option value={person.id}>{person.display_name}</option>
          {/each}
        </select>
        <button type="button" on:click={addChild} disabled={!selectedChild}>Add child 🌱</button>
      </div>
    </section>

    <section>
      <h2>📆 Events</h2>
      {#if detail.events.length === 0}
        <p>No family events yet — add one to capture milestones.</p>
      {:else}
        <ul class="list">
          {#each detail.events as event}
            <li>
              <button type="button" class="linkish" on:click={() => goto(withNavContext(`/events/${event.id}`))}>
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
      <h2>📚 Source citations</h2>
      <EvidenceTracePanel
        title="Evidence linked to this family"
        refs={evidenceRefs()}
        fromLabel={familyTitle()}
        fromHref={originHref()}
      />
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

  .core-edit {
    border: 1px solid #d8cff5;
    border-radius: 0.7rem;
    padding: 0.75rem;
    background: #fcf9ff;
  }

  .core-edit h2 {
    margin: 0 0 0.55rem;
    color: #5a3fa8;
    font-size: 0.95rem;
  }

  .core-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.5rem;
    margin-bottom: 0.5rem;
  }

  .core-grid label {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    font-size: 0.84rem;
    color: #5a4f7d;
  }

  .core-grid input,
  .core-grid select {
    border: 1px solid #d7cdf2;
    border-radius: 0.45rem;
    padding: 0.35rem 0.45rem;
    font: inherit;
    font-size: 0.85rem;
    min-width: 0;
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

  .back-link {
    align-self: flex-start;
    background: transparent;
    border: 0;
    color: #4c1d95;
    padding: 0;
    text-decoration: underline;
  }

  .small {
    padding: 0.2rem 0.45rem;
    font-size: 0.8rem;
    margin-left: 0.4rem;
  }

  .danger {
    background: #d03165;
  }

  .secondary {
    background: #5b6b83;
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
