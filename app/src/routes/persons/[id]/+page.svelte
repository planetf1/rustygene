<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { addRecentItem } from '$lib/state.svelte';
  import { api } from '$lib/api';
  import PersonForm from '$lib/components/PersonForm.svelte';
  import type { PersonDraft } from '$lib/components/formTypes';
  import AssertionList from '$lib/components/AssertionList.svelte';
  import NoteList from '$lib/components/NoteList.svelte';

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

  function familyLabel(family: FamilySummary): string {
    const left = family.partner1?.display_name ?? 'Unknown';
    const right = family.partner2?.display_name ?? 'Unknown';
    return `${left} + ${right}`;
  }

  function citationSourceTitle(citationId: string): string {
    return sourceMap.get(citationId) ?? citationId;
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
  <p>Loading person profile…</p>
{:else if error}
  <main class="panel">
    <h1>👤 Person profile</h1>
    <p class="error">{error}</p>
  </main>
{:else if detail}
  <main class="panel">
    {#if backHref}
      <button type="button" class="back-link" on:click={() => goto(backHref)}>{backLabel}</button>
    {/if}

    <header class="header">
      <div>
        <h1>{displayName()}</h1>
        <p>{lifeSummary()} · <span class="badge">{genderBadge()}</span></p>
      </div>
      <div class="actions">
        <button type="button" on:click={() => (showEdit = true)}>Edit profile</button>
        <button type="button" class="danger" on:click={removePerson} disabled={deleting}>
          {deleting ? 'Removing…' : 'Remove person'}
        </button>
      </div>
    </header>

    <section class="chart-links">
      <button type="button" class="pill" on:click={() => openInChart('pedigree')}>📊 Pedigree</button>
      <button type="button" class="pill" on:click={() => openInChart('fan')}>🔁 Fan chart</button>
      <button type="button" class="pill" on:click={() => openInChart('graph')}>🕸 Relationship graph</button>
    </section>

    <section>
      <h2>Summary</h2>
      <ul class="list">
        {#if birthEvent()}
          <li><strong>Birth:</strong> {formatDate(birthEvent()?.date)} {birthEvent()?.description ?? ''}</li>
        {/if}
        {#if deathEvent()}
          <li><strong>Death:</strong> {formatDate(deathEvent()?.date)} {deathEvent()?.description ?? ''}</li>
        {/if}
        {#if !birthEvent() && !deathEvent()}
          <li>Life events are not yet recorded.</li>
        {/if}
      </ul>
    </section>

    <section>
      <h2>🕰️ Timeline</h2>
      {#if timelineRows.length === 0}
        <p>No life events yet — add one to bring this timeline to life.</p>
      {:else}
        <ul class="list">
          {#each timelineRows as event (event.id)}
            <li>
              <button type="button" class="linkish" on:click={() => goto(`/events/${event.id}`)}>
                {event.event_type} — {event.date ? formatDate(event.date) : 'No date'}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <h2>🏡 Families</h2>
      {#if familyRows.length === 0}
        <p>No family links yet — you can connect relationships from a family page.</p>
      {:else}
        <ul class="list">
          {#each familyRows as family (family.id)}
            <li>
              <span>{familyLabel(family)} ({family.your_role ?? 'related'}) · </span>
              <button type="button" class="linkish" on:click={() => goto(`/families/${family.id}`)}>
                Open family
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <h2>📚 Sources & citations</h2>
      {#if flattenCitations().length === 0}
        <p>No citations linked yet — add evidence to strengthen this profile.</p>
      {:else}
        <ul class="list">
          {#each flattenCitations() as citation}
            <li><code>{citationSourceTitle(citation)}</code></li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <button type="button" class="secondary" on:click={() => (showEvidence = !showEvidence)}>
        {showEvidence ? 'Hide evidence details' : 'Show evidence details'}
      </button>
      {#if showEvidence}
        <div class="evidence-wrap">
          <AssertionList entityId={id} entityType="persons" assertions={assertionGroup} on:updated={loadDetail} />
        </div>
      {/if}
    </section>

    <section>
      <h2>📝 Notes</h2>
      <NoteList entityId={id} entityType="person" />
    </section>

    <section>
      <h2>🖼️ Media</h2>
      <p>Media gallery support is on deck — this section is ready when the endpoint lands.</p>
    </section>
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
    margin: 0.25rem 0 0;
    color: #6b6192;
  }

  .badge {
    background: #eef2ff;
    border: 1px solid #c7d2fe;
    border-radius: 999px;
    padding: 0.12rem 0.4rem;
    font-size: 0.8rem;
    color: #3730a3;
  }

  .chart-links {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
    border: 0;
    padding: 0;
    background: transparent;
  }

  .pill {
    background: #f8fafc;
    color: #0f172a;
    border: 1px solid #cbd5e1;
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
    gap: 0.3rem;
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

  button {
    background: #2563eb;
    color: #fff;
    border: 0;
    border-radius: 0.7rem;
    padding: 0.45rem 0.72rem;
    cursor: pointer;
  }

  .secondary {
    background: #7258c7;
  }

  .back-link {
    align-self: flex-start;
    background: transparent;
    border: 0;
    color: #4c1d95;
    padding: 0;
    text-decoration: underline;
  }

  .evidence-wrap {
    margin-top: 0.7rem;
  }

  .danger {
    background: #d03165;
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
