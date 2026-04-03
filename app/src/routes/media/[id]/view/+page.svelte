<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { page } from '$app/stores';
  import { api } from '$lib/api';

  type MediaDetail = {
    id: string;
    file_path: string;
    mime_type: string;
    caption: string | null;
    ocr_text: string | null;
    dimensions_px?: { width: number; height: number } | null;
  };

  type MediaLink = {
    entity_id: string;
    entity_type: string;
    display_name: string;
  };

  type NoteItem = {
    id: string;
    text: string;
    note_type: string;
    position_x_pct?: number | null;
    position_y_pct?: number | null;
  };

  type StagingSuggestion = {
    id: string;
    entity_type: string;
    entity_id: string;
    proposed_field: string;
    proposed_value: {
      entity_id?: string;
      entity_type?: string;
      display_name?: string;
      confidence?: number;
    };
    confidence: number;
    status: string;
    diff_summary: string;
  };

  type SearchResult = {
    entity_type: string;
    entity_id: string;
    display_name: string;
  };

  type SearchResponse = {
    results: SearchResult[];
  };

  type ViewerTab = 'ocr' | 'links' | 'suggested';

  let mediaId = '';
  let media: MediaDetail | null = null;
  let links: MediaLink[] = [];
  let annotations: NoteItem[] = [];
  let suggestions: StagingSuggestion[] = [];

  let loading = false;
  let savingText = false;
  let extracting = false;
  let busySuggestionId = '';
  let busyAttach = false;
  let error = '';
  let message = '';

  let activeTab: ViewerTab = 'ocr';
  let annotationPanelOpen = true;
  let ocrDraft = '';
  let pdfPage = 1;
  let zoomScale = 1;
  let panX = 0;
  let panY = 0;
  let isDragging = false;
  let dragStartX = 0;
  let dragStartY = 0;
  let dragOriginX = 0;
  let dragOriginY = 0;

  let annotationDraftText = '';
  let annotationX: number | null = null;
  let annotationY: number | null = null;

  let attachQuery = '';
  let attachResults: SearchResult[] = [];
  let attachEntityId = '';

  let eventSource: EventSource | null = null;

  function isImage(): boolean {
    return media?.mime_type.startsWith('image/') ?? false;
  }

  function isPdf(): boolean {
    return media?.mime_type === 'application/pdf';
  }

  function confidencePercent(value: number | undefined): number {
    return Math.round(Math.max(0, Math.min(1, value ?? 0)) * 100);
  }

  function imageUrl(): string {
    return api.url(`/api/v1/media/${mediaId}/file`);
  }

  function pdfUrl(): string {
    return `${api.url(`/api/v1/media/${mediaId}/file`)}#page=${pdfPage}`;
  }

  async function loadMedia(): Promise<void> {
    media = await api.get<MediaDetail>(`/api/v1/media/${mediaId}`);
    ocrDraft = media.ocr_text ?? '';
  }

  async function loadLinks(): Promise<void> {
    links = await api.get<MediaLink[]>(`/api/v1/media/${mediaId}/links`);
  }

  async function loadAnnotations(): Promise<void> {
    const rows = await api.get<NoteItem[]>(`/api/v1/notes?entity_id=${encodeURIComponent(mediaId)}`);
    annotations = rows.filter(
      (note) => note.position_x_pct !== null && note.position_x_pct !== undefined && note.position_y_pct !== null && note.position_y_pct !== undefined
    );
  }

  async function loadSuggestions(): Promise<void> {
    suggestions = await api.get<StagingSuggestion[]>(
      `/api/v1/staging?status=pending&entity_id=${encodeURIComponent(mediaId)}&entity_type=media`
    );
  }

  async function loadAll(): Promise<void> {
    if (!mediaId) {
      return;
    }

    loading = true;
    error = '';

    try {
      await Promise.all([loadMedia(), loadLinks(), loadAnnotations(), loadSuggestions()]);
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load document viewer';
    } finally {
      loading = false;
    }
  }

  function subscribeToUpdates(): void {
    if (eventSource) {
      eventSource.close();
    }

    eventSource = new EventSource(api.url('/api/v1/events/stream?types=entity.updated'));
    eventSource.addEventListener('entity.updated', (event) => {
      const payload = JSON.parse((event as MessageEvent<string>).data) as {
        entity_type: string;
        entity_id: string;
      };
      if (payload.entity_type === 'media' && payload.entity_id === mediaId) {
        void loadAll();
      }
    });
  }

  async function triggerExtract(): Promise<void> {
    extracting = true;
    error = '';
    message = '';
    try {
      await api.post(`/api/v1/media/${mediaId}/extract`, {});
      message = 'OCR extraction queued. Waiting for refresh event…';
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to trigger extraction';
    } finally {
      extracting = false;
    }
  }

  async function saveOcrText(): Promise<void> {
    const text = ocrDraft.trim();
    if (!text) {
      error = 'OCR text cannot be empty.';
      return;
    }

    savingText = true;
    error = '';
    message = '';
    try {
      await api.put(`/api/v1/media/${mediaId}/text`, { text });
      message = 'OCR text saved.';
      await loadMedia();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to save OCR text';
    } finally {
      savingText = false;
    }
  }

  function zoomBy(delta: number): void {
    zoomScale = Math.max(0.4, Math.min(4, zoomScale + delta));
  }

  function fitWidth(): void {
    zoomScale = 1;
    panX = 0;
    panY = 0;
  }

  function fitHeight(): void {
    zoomScale = 0.85;
    panX = 0;
    panY = 0;
  }

  function beginDrag(event: MouseEvent): void {
    if (!isImage()) {
      return;
    }
    isDragging = true;
    dragStartX = event.clientX;
    dragStartY = event.clientY;
    dragOriginX = panX;
    dragOriginY = panY;
  }

  function continueDrag(event: MouseEvent): void {
    if (!isDragging) {
      return;
    }
    panX = dragOriginX + (event.clientX - dragStartX);
    panY = dragOriginY + (event.clientY - dragStartY);
  }

  function endDrag(): void {
    isDragging = false;
  }

  function placeAnnotationAtPercent(x: number, y: number): void {
    annotationX = Math.max(0, Math.min(100, Math.round(x)));
    annotationY = Math.max(0, Math.min(100, Math.round(y)));
    annotationPanelOpen = true;
    if (!annotationDraftText) {
      annotationDraftText = 'Annotation';
    }
  }

  function placeAnnotation(event: MouseEvent): void {
    if (!isImage()) {
      return;
    }

    const element = event.currentTarget as HTMLDivElement;
    const rect = element.getBoundingClientRect();
    placeAnnotationAtPercent(
      ((event.clientX - rect.left) / rect.width) * 100,
      ((event.clientY - rect.top) / rect.height) * 100
    );
  }

  function handleImageStageKeydown(event: KeyboardEvent): void {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      placeAnnotationAtPercent(50, 50);
    }
  }

  async function saveAnnotation(): Promise<void> {
    const text = annotationDraftText.trim();
    if (!text || annotationX === null || annotationY === null) {
      error = 'Click the image and enter annotation text before saving.';
      return;
    }

    error = '';
    message = '';
    try {
      await api.post('/api/v1/notes', {
        text,
        note_type: 'Research',
        linked_entity_id: mediaId,
        linked_entity_type: 'media',
        position_x_pct: annotationX,
        position_y_pct: annotationY
      });
      message = 'Annotation saved.';
      annotationDraftText = '';
      annotationX = null;
      annotationY = null;
      await loadAnnotations();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to save annotation';
    }
  }

  async function deleteAnnotation(noteId: string): Promise<void> {
    const confirmed = confirm('Delete this annotation?');
    if (!confirmed) {
      return;
    }
    try {
      await api.del(`/api/v1/notes/${noteId}`);
      await loadAnnotations();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to delete annotation';
    }
  }

  async function searchAttachTargets(): Promise<void> {
    const query = attachQuery.trim();
    if (!query) {
      attachResults = [];
      return;
    }

    try {
      const response = await api.get<SearchResponse>(`/api/v1/search?q=${encodeURIComponent(query)}&limit=10`);
      attachResults = response.results.filter((result) => ['person', 'family', 'event'].includes(result.entity_type));
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to search entities';
    }
  }

  async function attachToEntity(result: SearchResult): Promise<void> {
    busyAttach = true;
    error = '';
    message = '';
    try {
      await api.post(`/api/v1/entities/${result.entity_id}/media/${mediaId}`, {});
      attachEntityId = result.entity_id;
      message = `Attached to ${result.display_name}.`;
      await Promise.all([loadLinks(), loadSuggestions()]);
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to attach media';
    } finally {
      busyAttach = false;
    }
  }

  async function reviewSuggestion(proposal: StagingSuggestion, action: 'approve' | 'reject'): Promise<void> {
    busySuggestionId = proposal.id;
    error = '';
    message = '';

    try {
      if (action === 'approve') {
        await api.post(`/api/v1/staging/${proposal.id}/approve`, { reviewer: 'viewer' });
        const entityId = proposal.proposed_value.entity_id;
        if (entityId) {
          await api.post(`/api/v1/entities/${entityId}/media/${mediaId}`, {});
        }
        message = `Accepted suggestion for ${proposal.proposed_value.display_name ?? entityId ?? proposal.id}.`;
        await loadLinks();
      } else {
        await api.post(`/api/v1/staging/${proposal.id}/reject`, { reviewer: 'viewer', reason: 'Rejected from document viewer' });
        message = 'Suggestion rejected.';
      }

      await loadSuggestions();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to review suggestion';
    } finally {
      busySuggestionId = '';
    }
  }

  function editSuggestion(proposal: StagingSuggestion): void {
    attachQuery = proposal.proposed_value.display_name ?? '';
    activeTab = 'links';
    void searchAttachTargets();
  }

  $: {
    const nextId = $page.params.id;
    if (nextId && nextId !== mediaId) {
      mediaId = nextId;
      void loadAll();
      subscribeToUpdates();
    }
  }

  onMount(async () => {
    if ($page.params.id) {
      mediaId = $page.params.id;
      await loadAll();
      subscribeToUpdates();
    }
  });

  onDestroy(() => {
    if (eventSource) {
      eventSource.close();
    }
  });
</script>

<main class="viewer-page">
  <header class="page-head">
    <div>
      <h1>{media?.caption ?? `Document ${mediaId}`}</h1>
      <p>{media?.mime_type ?? 'Loading…'}</p>
    </div>
    <div class="controls">
      {#if isImage()}
        <button type="button" class="secondary" on:click={() => zoomBy(-0.1)}>Zoom out</button>
        <button type="button" class="secondary" on:click={() => zoomBy(0.1)}>Zoom in</button>
        <button type="button" class="secondary" on:click={fitWidth}>Fit width</button>
        <button type="button" class="secondary" on:click={fitHeight}>Fit height</button>
      {:else if isPdf()}
        <button type="button" class="secondary" on:click={() => (pdfPage = Math.max(1, pdfPage - 1))}>Prev page</button>
        <span class="pdf-page">Page {pdfPage}</span>
        <button type="button" class="secondary" on:click={() => (pdfPage += 1)}>Next page</button>
      {/if}
    </div>
  </header>

  {#if message}
    <p class="ok">{message}</p>
  {/if}
  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if loading}
    <p>Loading document viewer…</p>
  {:else if media}
    <section class="split-layout">
      <section class="image-panel">
        {#if isImage()}
          <div
            class="image-stage"
            role="button"
            tabindex="0"
            aria-label="Document image viewer. Click or press Enter to place an annotation."
            on:mousedown={beginDrag}
            on:mousemove={continueDrag}
            on:mouseup={endDrag}
            on:mouseleave={endDrag}
            on:click={placeAnnotation}
            on:keydown={handleImageStageKeydown}
          >
            <img
              src={imageUrl()}
              alt={media.caption ?? media.id}
              draggable="false"
              style={`transform: translate(${panX}px, ${panY}px) scale(${zoomScale});`}
            />

            {#each annotations as note}
              <button
                type="button"
                class="pin"
                style={`left:${note.position_x_pct}%; top:${note.position_y_pct}%;`}
                title={note.text}
              >
                •
              </button>
            {/each}
          </div>
        {:else if isPdf()}
          <iframe class="pdf-frame" src={pdfUrl()} title={media.caption ?? media.id}></iframe>
        {:else}
          <div class="fallback-file">
            <p>This media type is not previewable inline.</p>
            <a href={imageUrl()} target="_blank" rel="noreferrer">Open original file</a>
          </div>
        {/if}
      </section>

      <section class="text-panel">
        <div class="tab-strip">
          <button type="button" class:active={activeTab === 'ocr'} on:click={() => (activeTab = 'ocr')}>OCR Text</button>
          <button type="button" class:active={activeTab === 'links'} on:click={() => (activeTab = 'links')}>Linked Assertions</button>
          <button type="button" class:active={activeTab === 'suggested'} on:click={() => (activeTab = 'suggested')}>Suggested Links</button>
        </div>

        {#if activeTab === 'ocr'}
          <section class="tab-card">
            {#if media.ocr_text}
              <label>
                OCR text
                <textarea rows="16" bind:value={ocrDraft}></textarea>
              </label>
              <div class="actions">
                <button type="button" on:click={saveOcrText} disabled={savingText}>
                  {savingText ? 'Saving…' : 'Save OCR text'}
                </button>
              </div>
            {:else}
              <p>No OCR text is stored for this media yet.</p>
              <button type="button" on:click={triggerExtract} disabled={extracting}>
                {extracting ? 'Queueing…' : 'Extract text'}
              </button>
            {/if}
          </section>
        {:else if activeTab === 'links'}
          <section class="tab-card">
            {#if links.length === 0}
              <p>No linked entities yet.</p>
            {:else}
              <ul class="list">
                {#each links as link}
                  <li>
                    <strong>{link.display_name}</strong>
                    <span>{link.entity_type} · {link.entity_id}</span>
                  </li>
                {/each}
              </ul>
            {/if}

            <div class="attach-box">
              <h3>Attach to…</h3>
              <div class="attach-row">
                <input type="search" bind:value={attachQuery} placeholder="Search people, families, or events" />
                <button type="button" class="secondary" on:click={searchAttachTargets} disabled={busyAttach}>Search</button>
              </div>

              {#if attachResults.length > 0}
                <ul class="list compact">
                  {#each attachResults as result}
                    <li>
                      <div>
                        <strong>{result.display_name}</strong>
                        <span>{result.entity_type} · {result.entity_id}</span>
                      </div>
                      <button type="button" on:click={() => attachToEntity(result)} disabled={busyAttach}>
                        Attach
                      </button>
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>
          </section>
        {:else}
          <section class="tab-card">
            {#if suggestions.length === 0}
              <p>No OCR-driven suggestions available yet.</p>
            {:else}
              <ul class="list">
                {#each suggestions as proposal}
                  <li>
                    <div>
                      <strong>{proposal.proposed_value.display_name ?? proposal.id}</strong>
                      <span>
                        {proposal.proposed_value.entity_type ?? proposal.entity_type}
                        · confidence {confidencePercent(proposal.proposed_value.confidence ?? proposal.confidence)}%
                      </span>
                    </div>
                    <div class="actions inline">
                      <button type="button" on:click={() => reviewSuggestion(proposal, 'approve')} disabled={busySuggestionId === proposal.id}>Accept</button>
                      <button type="button" class="secondary" on:click={() => editSuggestion(proposal)}>Edit</button>
                      <button type="button" class="danger" on:click={() => reviewSuggestion(proposal, 'reject')} disabled={busySuggestionId === proposal.id}>Reject</button>
                    </div>
                  </li>
                {/each}
              </ul>
            {/if}
          </section>
        {/if}

        <details bind:open={annotationPanelOpen} class="annotation-panel">
          <summary>Annotation panel</summary>
          <p>Click on the document image to place a marker, then save an annotation linked to this media item.</p>
          <div class="annotation-coords">
            <span>X: {annotationX ?? '—'}%</span>
            <span>Y: {annotationY ?? '—'}%</span>
          </div>
          <textarea rows="4" bind:value={annotationDraftText} placeholder="Annotation text"></textarea>
          <div class="actions">
            <button type="button" on:click={saveAnnotation}>Save annotation</button>
          </div>

          {#if annotations.length > 0}
            <ul class="list compact">
              {#each annotations as note}
                <li>
                  <div>
                    <strong>{note.text}</strong>
                    <span>{note.position_x_pct}% × {note.position_y_pct}%</span>
                  </div>
                  <button type="button" class="danger" on:click={() => deleteAnnotation(note.id)}>Delete</button>
                </li>
              {/each}
            </ul>
          {/if}
        </details>
      </section>
    </section>
  {/if}
</main>

<style>
  .viewer-page {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    background: #ffffff;
    border: 1px solid #e2e8f0;
    border-radius: 0.75rem;
    padding: 1rem;
  }

  .page-head,
  .controls,
  .actions,
  .attach-row,
  .annotation-coords,
  .tab-strip {
    display: flex;
    gap: 0.6rem;
    align-items: center;
    flex-wrap: wrap;
  }

  .page-head {
    justify-content: space-between;
  }

  .page-head h1,
  .page-head p,
  h3 {
    margin: 0;
  }

  .page-head p,
  .list span,
  .pdf-page,
  .annotation-coords,
  .tab-card p {
    color: #64748b;
  }

  .split-layout {
    display: grid;
    grid-template-columns: minmax(0, 3fr) minmax(20rem, 2fr);
    gap: 1rem;
  }

  .image-panel,
  .text-panel {
    border: 1px solid #e2e8f0;
    border-radius: 0.75rem;
    padding: 0.85rem;
    background: #f8fafc;
  }

  .image-stage {
    position: relative;
    overflow: hidden;
    min-height: 40rem;
    border-radius: 0.65rem;
    background: #0f172a;
    cursor: crosshair;
  }

  .image-stage img {
    width: 100%;
    height: auto;
    transform-origin: center center;
    user-select: none;
  }

  .pin {
    position: absolute;
    transform: translate(-50%, -50%);
    border: 0;
    width: 1.25rem;
    height: 1.25rem;
    border-radius: 999px;
    background: #ef4444;
    color: #ffffff;
    display: grid;
    place-items: center;
    font-size: 1rem;
    cursor: pointer;
  }

  .pdf-frame {
    width: 100%;
    min-height: 40rem;
    border: 1px solid #cbd5e1;
    border-radius: 0.65rem;
    background: #ffffff;
  }

  .fallback-file {
    min-height: 20rem;
    display: grid;
    place-items: center;
    text-align: center;
  }

  .tab-strip button.active {
    background: #1d4ed8;
    color: #ffffff;
  }

  .tab-card,
  .annotation-panel {
    background: #ffffff;
    border: 1px solid #e2e8f0;
    border-radius: 0.65rem;
    padding: 0.85rem;
    margin-top: 0.75rem;
  }

  .list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .list li {
    display: flex;
    justify-content: space-between;
    gap: 0.75rem;
    align-items: center;
    border: 1px solid #e2e8f0;
    border-radius: 0.55rem;
    padding: 0.65rem;
  }

  .compact li {
    padding: 0.45rem 0.55rem;
  }

  .attach-box {
    margin-top: 1rem;
    border-top: 1px solid #e2e8f0;
    padding-top: 1rem;
  }

  textarea,
  input {
    width: 100%;
    border: 1px solid #cbd5e1;
    border-radius: 0.45rem;
    padding: 0.55rem 0.65rem;
    font: inherit;
    box-sizing: border-box;
  }

  button {
    border: 0;
    border-radius: 0.45rem;
    padding: 0.5rem 0.8rem;
    background: #2563eb;
    color: #ffffff;
    cursor: pointer;
    width: fit-content;
  }

  .secondary {
    background: #475569;
  }

  .danger {
    background: #b91c1c;
  }

  .inline {
    justify-content: flex-end;
  }

  .ok,
  .error {
    margin: 0;
  }

  .ok {
    color: #166534;
  }

  .error {
    color: #b91c1c;
  }

  @media (max-width: 1100px) {
    .split-layout {
      grid-template-columns: 1fr;
    }

    .image-stage,
    .pdf-frame {
      min-height: 28rem;
    }
  }
</style>