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

  function flattenCitations(): string[] {
    const names = detail?.names ?? [];
    const values = names.flatMap((name) => name.sources ?? []);
    if (values.length === 0) {
      return [];
    }

    return values.map((source, index) => source.citation_id ?? source.source_id ?? `citation-${index + 1}`);
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
    <header class="header">
      <div>
        <h1>
          {detail.names[0]?.given_names.join(' ')}
          {detail.names[0]?.surnames.map((surname) => surname.value).join(' ')}
        </h1>
        <p>ID: <code>{id}</code></p>
      </div>
      <div class="actions">
        <button type="button" on:click={() => (showEdit = true)}>Edit profile</button>
        <button type="button" class="danger" on:click={removePerson} disabled={deleting}>
          {deleting ? 'Removing…' : 'Remove person'}
        </button>
      </div>
    </header>

    <section>
      <AssertionList entityId={id} entityType="persons" assertions={assertionGroup} on:updated={loadDetail} />
      <button type="button" class="secondary" on:click={() => (showEdit = true)}>Tidy assertions ✨</button>
    </section>

    <section>
      <h2>🕰️ Timeline</h2>
      {#if detail.events.length === 0}
        <p>No life events yet — add one to bring this timeline to life.</p>
      {:else}
        <ul class="list">
          {#each detail.events as event}
            <li>
              <button type="button" class="linkish" on:click={() => goto(`/events/${event.id}`)}>
                {event.event_type} — {event.date ? JSON.stringify(event.date) : 'No date'}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <h2>🏡 Families</h2>
      {#if detail.families.length === 0}
        <p>No family links yet — you can connect relationships from a family page.</p>
      {:else}
        <ul class="list">
          {#each detail.families as family}
            <li>
              <button type="button" class="linkish" on:click={() => goto(`/families/${family.id}`)}>
                Open family {family.id} ({family.your_role ?? 'related'})
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
            <li><code>{citation}</code></li>
          {/each}
        </ul>
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
