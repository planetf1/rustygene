<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { addRecentItem } from '$lib/state.svelte';
  import { api } from '$lib/api';
  import PersonForm from '$lib/components/PersonForm.svelte';
  import type { PersonDraft } from '$lib/components/formTypes';
  import AssertionList from '$lib/components/AssertionList.svelte';
  import BreadcrumbTrail from '$lib/components/BreadcrumbTrail.svelte';
  import NoteList from '$lib/components/NoteList.svelte';
  import RelatedRecordsGraph from '$lib/components/RelatedRecordsGraph.svelte';

  type PersonNameAssertion = {
    assertion_id: string;
    given_names: string[];
    surnames: { value: string; origin_type: string; connector: string | null }[];
    name_type: string | null;
    sort_as: string | null;
    call_name: string | null;
    confidence: number;
    sources: { citation_id?: string; source_id?: string }[];
  };

  type GenderAssertion = {
    assertion_id: string;
    value: string;
    confidence: number;
  };

  type TimelineEvent = {
    id: string;
    event_type: string;
    date: unknown;
    description: string | null;
  };

  type SourceListItem = {
    id: string;
    title: string;
  };

  type FamilySummary = {
    id: string;
    partner1?: { id: string; display_name: string };
    partner2?: { id: string; display_name: string };
    your_role?: string;
  };

  type PersonDetail = {
    id: string;
    names: PersonNameAssertion[];
    gender_assertions: GenderAssertion[];
    events: TimelineEvent[];
    families: FamilySummary[];
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

  let detail: PersonDetail | null = null;
  let assertionGroup: AssertionGroup = {};
  let loading = false;
  let error = '';
  let showEdit = false;
  let deleting = false;
  let sourceMap = new Map<string, string>();
  let showEvidence = false;
  let backLabel = '';
  let backHref = '';
  let editingGender = false;
  let genderDraft = 'Unknown';
  let genderConfidence = 0.9;
  let genderEvidenceType: 'direct' | 'indirect' | 'negative' = 'direct';
  let genderError = '';
  let savingGender = false;
  $: timelineRows = detail?.events ?? [];
  $: familyRows = detail?.families ?? [];

  function flattenCitations(): string[] {
    const names = detail?.names ?? [];
    const values = names.flatMap((name) => name.sources ?? []);
    if (values.length === 0) {
      return [];
    }

    return values.map((source, index) => source.citation_id ?? source.source_id ?? `citation-${index + 1}`);
  }

  function citationTokensFromNames(names: PersonNameAssertion[]): string[] {
    const values = names.flatMap((name) => name.sources ?? []);
    return values
      .map((source, index) => source.source_id ?? source.citation_id ?? `citation-${index + 1}`)
      .filter((token) => Boolean(token));
  }

  function formatDate(value: unknown): string {
    if (value === null || value === undefined) {
      return 'unknown';
    }

    if (typeof value === 'string') {
      return value || 'unknown';
    }

    if (typeof value === 'object') {
      const record = value as Record<string, unknown>;
      const kind = String(record.type ?? '').toLowerCase();
      if (kind === 'textual') {
        return String(record.value ?? 'unknown');
      }

      if (kind === 'exact') {
        return String(record.date ?? 'unknown');
      }

      if (kind === 'between' || kind === 'range') {
        const from = String(record.from ?? record.start ?? '?');
        const to = String(record.to ?? record.end ?? '?');
        return `${from} to ${to}`;
      }
    }

    return JSON.stringify(value);
  }

  function displayName(): string {
    const given = detail?.names[0]?.given_names.join(' ') ?? '';
    const surname = detail?.names[0]?.surnames.map((s) => s.value).join(' ') ?? '';
    const joined = `${given} ${surname}`.trim();
    return joined || `Person ${id}`;
  }

  function birthEvent(): TimelineEvent | null {
    return detail?.events.find((event) => event.event_type.toLowerCase() === 'birth') ?? null;
  }

  function deathEvent(): TimelineEvent | null {
    return detail?.events.find((event) => event.event_type.toLowerCase() === 'death') ?? null;
  }

  function lifeSummary(): string {
    const birth = birthEvent();
    const death = deathEvent();
    const from = birth ? formatDate(birth.date) : '?';
    const to = death ? formatDate(death.date) : '?';
    return `b. ${from} — d. ${to}`;
  }

  function genderBadge(): string {
    return detail?.gender_assertions[0]?.value ?? 'Unknown';
  }

  function openGenderEdit(): void {
    genderDraft = genderBadge();
    genderConfidence = detail?.gender_assertions[0]?.confidence ?? 0.9;
    genderEvidenceType = 'direct';
    genderError = '';
    editingGender = true;
  }

  function cancelGenderEdit(): void {
    editingGender = false;
    genderError = '';
  }

  async function saveGenderEdit(): Promise<void> {
    if (!genderDraft.trim()) {
      genderError = 'Gender is required.';
      return;
    }

    savingGender = true;
    genderError = '';
    try {
      await api.post(`/api/v1/persons/${id}/assertions`, {
        field: 'gender',
        value: genderDraft.trim(),
        confidence: genderConfidence,
        evidence_type: genderEvidenceType,
        status: 'proposed'
      });
      editingGender = false;
      await loadDetail();
    } catch (err) {
      genderError = err instanceof Error ? err.message : 'Failed to save gender';
    } finally {
      savingGender = false;
    }
  }

  function familyLabel(family: FamilySummary): string {
    const left = family.partner1?.display_name ?? 'Unknown';
    const right = family.partner2?.display_name ?? 'Unknown';
    return `${left} + ${right}`;
  }

  function citationSourceTitle(citationId: string): string {
    return sourceMap.get(citationId) ?? citationId;
  }

  function originHref(): string {
    return `/persons/${id}`;
  }

  function originLabel(): string {
    return displayName();
  }

  function withNavContext(target: string): string {
    const current = `${$page.url.pathname}${$page.url.search}`;
    const sep = target.includes('?') ? '&' : '?';
    return `${target}${sep}from=${encodeURIComponent(originLabel())}&back=${encodeURIComponent(current)}`;
  }

  function breadcrumbItems(): Array<{ label: string; href?: string }> {
    const items: Array<{ label: string; href?: string }> = [{ label: 'Persons', href: '/persons' }];
    if (backHref && backHref !== '/persons') {
      items.push({ label: backLabel.replace('← Back to ', ''), href: backHref });
    }
    items.push({ label: displayName() });
    return items;
  }

  function relatedNodes(): Array<{ id: string; label: string; href: string; kind: 'person' | 'family' | 'event' | 'source' | 'citation' | 'repository' | 'media' | 'other' }> {
    const familyNodes = familyRows.map((family) => ({
      id: `family-${family.id}`,
      label: familyLabel(family),
      href: withNavContext(`/families/${family.id}`),
      kind: 'family' as const
    }));

    const eventNodes = timelineRows.map((event) => ({
      id: `event-${event.id}`,
      label: event.event_type,
      href: withNavContext(`/events/${event.id}`),
      kind: 'event' as const
    }));

    const sourceNodes = flattenCitations().map((citationId) => ({
      id: `source-${citationId}`,
      label: citationSourceTitle(citationId),
      href: withNavContext(`/sources/${citationId}`),
      kind: 'source' as const
    }));

    return [...familyNodes, ...eventNodes, ...sourceNodes].slice(0, 16);
  }

  function openInChart(mode: 'pedigree' | 'fan' | 'graph'): void {
    const name = displayName();

    if (mode === 'pedigree') {
      localStorage.setItem('pedigree_root_person_id', id);
      localStorage.setItem('pedigree_root_person_name', name);
      localStorage.setItem('navigation_context', JSON.stringify({ from: 'person', to: 'pedigree', personId: id }));
      void goto('/charts/pedigree');
      return;
    }

    if (mode === 'fan') {
      localStorage.setItem('ancestor_chart_root_person_id', id);
      localStorage.setItem('ancestor_chart_root_person_name', name);
      localStorage.setItem('navigation_context', JSON.stringify({ from: 'person', to: 'fan', personId: id }));
      void goto('/charts/fan');
      return;
    }

    localStorage.setItem('graph_center_person_id', id);
    localStorage.setItem('graph_center_person_name', name);
    localStorage.setItem('navigation_context', JSON.stringify({ from: 'person', to: 'graph', personId: id }));
    void goto('/charts/graph');
  }

  function readBackContext(): void {
    if (typeof window === 'undefined') {
      return;
    }

    const raw = localStorage.getItem('person_nav_context');
    if (!raw) {
      backLabel = '';
      backHref = '';
      return;
    }

    try {
      const parsed = JSON.parse(raw) as { from?: string; href?: string };
      const from = parsed.from ?? '';
      const href = parsed.href ?? '';
      if (!from || !href) {
        backLabel = '';
        backHref = '';
        return;
      }

      backLabel = `← Back to ${from}`;
      backHref = href;
    } catch {
      backLabel = '';
      backHref = '';
    }
  }

  function dateSortKey(value: unknown): string {
    if (!value) {
      return '9999';
    }

    if (typeof value === 'string') {
      return value;
    }

    return JSON.stringify(value);
  }

  function sortedTimeline(events: TimelineEvent[]): TimelineEvent[] {
    return [...events].sort((a, b) => dateSortKey(a.date).localeCompare(dateSortKey(b.date)));
  }

  function toEditDraft(): PersonDraft | null {
    const first = detail?.names[0];
    if (!first) {
      return null;
    }

    return {
      id,
      givenNames: first.given_names.length ? first.given_names : [''],
      surnames:
        first.surnames.length > 0
          ? first.surnames.map((surname) => ({
              value: surname.value,
              originType: (surname.origin_type as PersonDraft['surnames'][number]['originType']) ?? 'Unknown',
              connector: surname.connector ?? ''
            }))
          : [{ value: '', originType: 'Unknown', connector: '' }],
      nameType: (first.name_type as PersonDraft['nameType']) ?? 'Birth',
      sortAs: first.sort_as ?? '',
      callName: first.call_name ?? '',
      gender: (detail?.gender_assertions[0]?.value as PersonDraft['gender']) ?? 'Unknown',
      birthDate: '',
      birthPlace: '',
      deathDate: '',
      deathPlace: '',
      notes: '',
      citations: []
    };
  }

  async function loadDetail(): Promise<void> {
    loading = true;
    error = '';

    try {
      const [personDetail, assertions, timeline, families] = await Promise.all([
        api.get<PersonDetail>(`/api/v1/persons/${id}`),
        api.get<AssertionGroup>(`/api/v1/persons/${id}/assertions`),
        api.get<TimelineEvent[]>(`/api/v1/persons/${id}/timeline`),
        api.get<FamilySummary[]>(`/api/v1/persons/${id}/families`)
      ]);

      const citationIds = citationTokensFromNames(personDetail.names);
      if (citationIds.length > 0) {
        const ids = [...new Set(citationIds)];
        const sourceRows = await Promise.all(
          ids.map(async (citationId) => {
            try {
              const source = await api.get<SourceListItem>(`/api/v1/sources/${citationId}`);
              return [citationId, source.title] as const;
            } catch {
              return [citationId, citationId] as const;
            }
          })
        );
        sourceMap = new Map(sourceRows);
      } else {
        sourceMap = new Map();
      }

      detail = {
        ...personDetail,
        events: sortedTimeline(timeline),
        families
      };
      assertionGroup = assertions;

      const displayName =
        personDetail.names[0]?.given_names.join(' ') ||
        personDetail.names[0]?.surnames.map((surname) => surname.value).join(' ') ||
        `Person ${id}`;

      addRecentItem({
        entityType: 'person',
        id,
        displayName
      });
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load person detail';
    } finally {
      loading = false;
    }
  }

  async function removePerson(): Promise<void> {
    const confirmed = confirm('Delete this person? This cannot be undone.');
    if (!confirmed) {
      return;
    }

    deleting = true;
    error = '';

    try {
      await api.del(`/api/v1/persons/${id}`);
      await goto('/persons');
    } catch (err) {
      error = err instanceof Error ? err.message : 'Delete failed';
    } finally {
      deleting = false;
    }
  }

  onMount(async () => {
    readBackContext();
    await loadDetail();
  });
</script>

{#if loading}
  <p class="loading-msg">Loading person profile…</p>
{:else if error}
  <main class="panel">
    <h1>Person profile</h1>
    <p class="error">{error}</p>
  </main>
{:else if detail}
  <main class="panel">
    <BreadcrumbTrail items={breadcrumbItems()} />

    {#if backHref}
      <button type="button" class="back-link" on:click={() => goto(backHref)}>{backLabel}</button>
    {/if}

    <!-- ── Overview card ──────────────────────────────────────── -->
    <div class="overview-card">
      <div class="overview-left">
        <h1 class="overview-name">{displayName()}</h1>
        <p class="overview-lifespan">{lifeSummary()}</p>
        <div class="overview-badges">
          <span class="badge">{genderBadge()}</span>
          {#if detail.names[0]?.confidence}
            <span class="badge badge-confidence">Confidence {Math.round(detail.names[0].confidence * 100)}%</span>
          {/if}
          {#each (() => {
            const pts = [
              detail.names.length > 0,
              !!birthEvent(),
              !!deathEvent(),
              detail.gender_assertions.length > 0,
              flattenCitations().length > 0
            ];
            const score = pts.filter(Boolean).length;
            return [{ score, total: pts.length }];
          })() as c}
            <span class="badge badge-completeness {c.score === c.total ? 'badge-complete' : c.score > 2 ? 'badge-partial' : 'badge-low'}">
              {c.score}/{c.total} fields
            </span>
          {/each}
        </div>
      </div>
      <div class="overview-actions">
        <button type="button" class="btn-secondary" on:click={() => (editingGender ? cancelGenderEdit() : openGenderEdit())}>
          {editingGender ? 'Cancel gender edit' : 'Quick edit gender'}
        </button>
        <button type="button" class="btn-primary" on:click={() => (showEdit = true)}>Edit</button>
        <button type="button" class="btn-secondary" on:click={() => openInChart('pedigree')}>Pedigree</button>
        <button type="button" class="btn-secondary" on:click={() => openInChart('fan')}>Fan</button>
        <button type="button" class="btn-secondary" on:click={() => openInChart('graph')}>Graph</button>
        <button type="button" class="btn-danger" on:click={removePerson} disabled={deleting}>
          {deleting ? '…' : 'Delete'}
        </button>
      </div>
    </div>

    <section class="section-card">
      <RelatedRecordsGraph centerLabel={displayName()} nodes={relatedNodes()} />
    </section>

    <!-- ── Key facts ──────────────────────────────────────────── -->
    <section class="section-card">
      <h2 class="section-title">Key facts</h2>
      {#if editingGender}
        <div class="inline-edit">
          <label>
            Gender
            <select bind:value={genderDraft}>
              <option value="Male">Male</option>
              <option value="Female">Female</option>
              <option value="Unknown">Unknown</option>
            </select>
          </label>
          <label>
            Confidence
            <input type="number" min="0" max="1" step="0.01" bind:value={genderConfidence} />
          </label>
          <label>
            Evidence type
            <select bind:value={genderEvidenceType}>
              <option value="direct">Direct</option>
              <option value="indirect">Indirect</option>
              <option value="negative">Negative</option>
            </select>
          </label>
          <button type="button" class="btn-primary" on:click={saveGenderEdit} disabled={savingGender}>{savingGender ? 'Saving…' : 'Save gender'}</button>
          <button type="button" class="btn-secondary" on:click={cancelGenderEdit}>Cancel</button>
          {#if genderError}
            <p class="error">{genderError}</p>
          {/if}
        </div>
      {/if}
      <dl class="fact-grid">
        <dt>Birth</dt>
        <dd>
          {birthEvent() ? formatDate(birthEvent()?.date) : '—'}
          {#if birthEvent()?.description}<span class="fact-note">{birthEvent()?.description}</span>{/if}
        </dd>
        <dt>Death</dt>
        <dd>
          {deathEvent() ? formatDate(deathEvent()?.date) : '—'}
          {#if deathEvent()?.description}<span class="fact-note">{deathEvent()?.description}</span>{/if}
        </dd>
        <dt>Gender</dt>
        <dd>{genderBadge()}</dd>
        {#if detail.names.length > 1}
          <dt>Alt. names</dt>
          <dd>
            {detail.names.slice(1).map(n => [...n.given_names, ...n.surnames.map(s => s.value)].join(' ')).join('; ')}
          </dd>
        {/if}
        {#if familyRows.length > 0}
          <dt>Families</dt>
          <dd>
            <ul class="inline-list">
              {#each familyRows as family (family.id)}
                <li>
                  <button type="button" class="linkish" on:click={() => goto(withNavContext(`/families/${family.id}`))}>
                    {familyLabel(family)}
                  </button>
                  <span class="fact-note">({family.your_role ?? 'related'})</span>
                </li>
              {/each}
            </ul>
          </dd>
        {/if}
      </dl>
    </section>

    <!-- ── Timeline ───────────────────────────────────────────── -->
    <details class="section-card" open>
      <summary class="section-title section-toggle">
        Timeline
        <span class="count-badge">{timelineRows.length}</span>
      </summary>
      {#if timelineRows.length === 0}
        <p class="section-empty">No life events recorded yet.</p>
      {:else}
        <ul class="timeline-list">
          {#each timelineRows as event (event.id)}
            <li class="timeline-item">
              <span class="timeline-type">{event.event_type}</span>
              <span class="timeline-date">{event.date ? formatDate(event.date) : 'No date'}</span>
              {#if event.description}
                <span class="timeline-desc">{event.description}</span>
              {/if}
              <button type="button" class="linkish timeline-link" on:click={() => goto(withNavContext(`/events/${event.id}`))}>→</button>
            </li>
          {/each}
        </ul>
      {/if}
    </details>

    <!-- ── Sources & evidence ─────────────────────────────────── -->
    <details class="section-card">
      <summary class="section-title section-toggle">
        Sources &amp; evidence
        <span class="count-badge">{flattenCitations().length}</span>
      </summary>
      {#if flattenCitations().length === 0}
        <p class="section-empty">No citations linked yet — add evidence to strengthen this profile.</p>
      {:else}
        <ul class="list">
          {#each flattenCitations() as citation}
            <li>
              <button type="button" class="linkish" on:click={() => goto(withNavContext(`/sources/${citation}`))}>
                {citationSourceTitle(citation)}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
      <div class="evidence-toggle-wrap">
        <button type="button" class="btn-secondary" on:click={() => (showEvidence = !showEvidence)}>
          {showEvidence ? 'Hide assertion details' : 'Show assertion details'}
        </button>
        {#if showEvidence}
          <div class="evidence-wrap">
            <AssertionList entityId={id} entityType="persons" assertions={assertionGroup} on:updated={loadDetail} />
          </div>
        {/if}
      </div>
    </details>

    <!-- ── Notes ──────────────────────────────────────────────── -->
    <details class="section-card">
      <summary class="section-title section-toggle">Notes</summary>
      <NoteList entityId={id} entityType="person" />
    </details>
  </main>

  {#if showEdit}
    <button type="button" class="overlay" aria-label="Close person edit panel" on:click={() => (showEdit = false)}></button>
    <aside class="slideover">
      <PersonForm
        mode="edit"
        initial={toEditDraft()}
        on:cancel={() => (showEdit = false)}
        on:saved={(event: CustomEvent<{ id: string }>) => {
          showEdit = false;
          void goto(`/persons/${event.detail.id}`);
          void loadDetail();
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
    gap: 0.85rem;
  }

  /* ── Overview card ── */
  .overview-card {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 1rem;
    background: linear-gradient(135deg, #f3edff 0%, #fff0fd 100%);
    border: 1px solid #dfd2f8;
    border-radius: 0.85rem;
    padding: 1rem 1.15rem;
    flex-wrap: wrap;
  }

  .overview-name {
    margin: 0 0 0.2rem;
    font-size: 1.4rem;
    color: #3b1d8c;
  }

  .overview-lifespan {
    margin: 0 0 0.5rem;
    color: #6b5fa0;
    font-size: 0.95rem;
  }

  .overview-badges {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }

  .overview-actions {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    flex-wrap: wrap;
  }

  /* ── Badges ── */
  .badge {
    display: inline-block;
    background: #eef2ff;
    border: 1px solid #c7d2fe;
    border-radius: 999px;
    padding: 0.12rem 0.5rem;
    font-size: 0.78rem;
    color: #3730a3;
    font-weight: 500;
  }

  .badge-confidence {
    background: #f0fdf4;
    border-color: #86efac;
    color: #15803d;
  }

  .badge-complete {
    background: #f0fdf4;
    border-color: #86efac;
    color: #15803d;
  }

  .badge-partial {
    background: #fffbeb;
    border-color: #fcd34d;
    color: #92400e;
  }

  .badge-low {
    background: #fef2f2;
    border-color: #fca5a5;
    color: #991b1b;
  }

  /* ── Section cards ── */
  .section-card {
    border: 1px solid #efe6ff;
    border-radius: 0.75rem;
    padding: 0.85rem 1rem;
    background: #fffdff;
  }

  .section-title {
    margin: 0 0 0.65rem;
    font-size: 0.9rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: #593ca8;
  }

  .section-toggle {
    cursor: pointer;
    list-style: none;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0;
  }

  details.section-card[open] .section-toggle {
    margin-bottom: 0.65rem;
  }

  .section-toggle::-webkit-details-marker { display: none; }
  .section-toggle::before {
    content: '▶';
    font-size: 0.7rem;
    color: #9d84d8;
    transition: transform 0.15s;
  }

  details[open] > .section-toggle::before { transform: rotate(90deg); }

  .count-badge {
    background: #ede5ff;
    color: #5b21b6;
    border-radius: 999px;
    padding: 0.05rem 0.45rem;
    font-size: 0.75rem;
    font-weight: 600;
  }

  .section-empty {
    color: #888;
    font-style: italic;
    margin: 0.5rem 0 0;
    font-size: 0.9rem;
  }

  /* ── Key facts grid ── */
  .fact-grid {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.3rem 1rem;
    font-size: 0.9rem;
    margin: 0;
  }

  .inline-edit {
    margin: 0 0 0.7rem;
    border: 1px solid #dfd2f8;
    border-radius: 0.6rem;
    padding: 0.55rem;
    background: #fcf9ff;
    display: flex;
    align-items: flex-end;
    gap: 0.45rem;
    flex-wrap: wrap;
  }

  .inline-edit label {
    display: flex;
    flex-direction: column;
    gap: 0.18rem;
    font-size: 0.8rem;
    color: #5a4f7d;
  }

  .inline-edit input,
  .inline-edit select {
    border: 1px solid #d7cdf2;
    border-radius: 0.45rem;
    padding: 0.3rem 0.45rem;
    font: inherit;
    font-size: 0.84rem;
  }

  dt {
    font-weight: 600;
    color: #5a4f7d;
    white-space: nowrap;
  }

  dd {
    margin: 0;
    color: #1e1037;
  }

  .fact-note {
    color: #888;
    font-size: 0.82rem;
    margin-left: 0.25rem;
  }

  .inline-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }

  /* ── Timeline ── */
  .timeline-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0;
  }

  .timeline-item {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    padding: 0.35rem 0;
    border-bottom: 1px solid #f0e8ff;
    font-size: 0.9rem;
  }

  .timeline-item:last-child { border-bottom: 0; }

  .timeline-type {
    font-weight: 600;
    color: #4c1d95;
    min-width: 7rem;
  }

  .timeline-date {
    color: #6b5fa0;
    min-width: 7rem;
  }

  .timeline-desc {
    color: #555;
    flex: 1;
    font-size: 0.85rem;
  }

  .timeline-link {
    border: 0;
    background: transparent;
    color: #7c3aed;
    cursor: pointer;
    padding: 0;
    font: inherit;
    font-size: 0.9rem;
  }

  /* ── Buttons ── */
  .btn-primary {
    background: #6d28d9;
    color: #fff;
    border: 0;
    border-radius: 0.45rem;
    padding: 0.38rem 0.7rem;
    cursor: pointer;
    font-size: 0.85rem;
    font-weight: 600;
  }

  .btn-primary:hover { background: #5b21b6; }

  .btn-secondary {
    background: #f3edff;
    color: #5b21b6;
    border: 1px solid #dfd2f8;
    border-radius: 0.45rem;
    padding: 0.35rem 0.65rem;
    cursor: pointer;
    font-size: 0.85rem;
    font-weight: 500;
  }

  .btn-secondary:hover { background: #ede5ff; }

  .btn-danger {
    background: #fef2f2;
    color: #991b1b;
    border: 1px solid #fca5a5;
    border-radius: 0.45rem;
    padding: 0.35rem 0.65rem;
    cursor: pointer;
    font-size: 0.85rem;
  }

  .btn-danger:hover { background: #fee2e2; }
  .btn-danger:disabled { opacity: 0.5; cursor: default; }

  .back-link {
    align-self: flex-start;
    background: transparent;
    border: 0;
    color: #4c1d95;
    padding: 0;
    cursor: pointer;
    font: inherit;
    text-decoration: underline;
    font-size: 0.85rem;
  }

  .linkish {
    border: 0;
    background: transparent;
    color: #6a46dc;
    cursor: pointer;
    padding: 0;
    font: inherit;
    font-weight: 600;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .list {
    margin: 0.5rem 0 0;
    padding-left: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.9rem;
  }

  .evidence-toggle-wrap {
    margin-top: 0.75rem;
  }

  .evidence-wrap {
    margin-top: 0.65rem;
  }

  .loading-msg {
    color: #888;
    margin: 0;
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
    border-left: 1px solid var(--rg-border, #e8def8);
    padding: 1rem;
    overflow: auto;
  }

  .error {
    color: #b91c1c;
    margin: 0;
  }

</style>
