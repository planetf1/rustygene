<script lang="ts">
  import { goto } from '$app/navigation';
  import CitationDetail from '$lib/components/CitationDetail.svelte';

  type EvidenceRef = {
    citation_id?: string;
    source_id?: string;
    note?: string;
    label?: string;
  };

  export let title = 'Evidence';
  export let refs: EvidenceRef[] = [];
  export let fromLabel = '';
  export let fromHref = '';

  let activeCitationId = '';
  let activeCitationNote = '';

  $: uniqueRefs = dedupe(refs);

  function dedupe(items: EvidenceRef[]): EvidenceRef[] {
    const seen = new Set<string>();
    const result: EvidenceRef[] = [];

    for (const ref of items) {
      const key = `${ref.citation_id ?? ''}|${ref.source_id ?? ''}|${ref.note ?? ''}|${ref.label ?? ''}`;
      if (seen.has(key)) {
        continue;
      }
      seen.add(key);
      result.push(ref);
    }

    return result;
  }

  function navQuery(): string {
    if (!fromLabel || !fromHref) {
      return '';
    }

    return `?from=${encodeURIComponent(fromLabel)}&back=${encodeURIComponent(fromHref)}`;
  }

  function openCitation(citationId: string): void {
    void goto(`/citations/${citationId}${navQuery()}`);
  }

  function openSource(sourceId: string): void {
    void goto(`/sources/${sourceId}${navQuery()}`);
  }

  function previewCitation(citationId: string | undefined, note?: string): void {
    if (!citationId) {
      activeCitationId = '';
      activeCitationNote = '';
      return;
    }
    activeCitationId = citationId;
    activeCitationNote = note ?? '';
  }
</script>

<section class="evidence-panel">
  <header class="panel-head">
    <h3>{title}</h3>
    <span class="count">{uniqueRefs.length}</span>
  </header>

  {#if uniqueRefs.length === 0}
    <p class="empty">No evidence linked yet.</p>
  {:else}
    <ul class="evidence-list">
      {#each uniqueRefs as ref}
        <li
          class="evidence-row"
          on:mouseenter={() => previewCitation(ref.citation_id, ref.note)}
          on:focusin={() => previewCitation(ref.citation_id, ref.note)}
          on:mouseleave={() => previewCitation(undefined)}
        >
          <span class="label">{ref.label ?? 'Fact evidence'}</span>
          <div class="chips">
            {#if ref.citation_id}
              <button type="button" class="chip citation" on:click={() => openCitation(ref.citation_id ?? '')}>
                {ref.citation_id}
              </button>
            {/if}
            {#if ref.source_id}
              <button type="button" class="chip source" on:click={() => openSource(ref.source_id ?? '')}>
                source:{ref.source_id}
              </button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  {/if}

  {#if activeCitationId}
    <div class="preview">
      <CitationDetail citationId={activeCitationId} citationNote={activeCitationNote} />
    </div>
  {/if}
</section>

<style>
  .evidence-panel {
    border: 1px solid #e5def8;
    border-radius: 0.75rem;
    padding: 0.8rem;
    background: #fff;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .panel-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }

  .panel-head h3 {
    margin: 0;
    font-size: 0.88rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: #593ca8;
  }

  .count {
    background: #ede5ff;
    color: #5b21b6;
    border-radius: 999px;
    padding: 0.08rem 0.4rem;
    font-size: 0.75rem;
    font-weight: 600;
  }

  .empty {
    margin: 0;
    color: #888;
    font-style: italic;
    font-size: 0.9rem;
  }

  .evidence-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .evidence-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.35rem 0.45rem;
    border: 1px solid #f0e8ff;
    border-radius: 0.45rem;
    background: #fffdff;
  }

  .evidence-row:hover {
    background: #f9f4ff;
    border-color: #dfd2f8;
  }

  .label {
    font-size: 0.84rem;
    color: #4a3e71;
  }

  .chips {
    display: flex;
    gap: 0.3rem;
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .chip {
    border-radius: 999px;
    border: 1px solid #c4b5fd;
    padding: 0.12rem 0.45rem;
    font-size: 0.75rem;
    cursor: pointer;
    background: #f5f3ff;
    color: #5b21b6;
  }

  .chip.source {
    border-color: #a7f3d0;
    background: #ecfdf5;
    color: #065f46;
  }

  .chip:hover {
    filter: brightness(0.98);
  }

  .preview {
    margin-top: 0.1rem;
  }
</style>
