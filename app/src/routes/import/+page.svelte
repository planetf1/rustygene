<script lang="ts">
  import { goto } from '$app/navigation';
  import { onDestroy, onMount } from 'svelte';
  import { api } from '$lib/api';
  import {
    detectImportFormat,
    formatLabel,
    mergeUniqueLogMessages,
    normalizeEntityCounts,
    reportCards,
    type ImportCompletedEvent,
    type ImportFormat,
    type ImportJobStatus
  } from '$lib/importWizard';

  type TauriInvoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

  type ImportAcceptedResponse = {
    job_id: string;
    status_url: string;
  };

  const formatOptions: Array<{ value: ImportFormat; label: string }> = [
    { value: 'gedcom', label: formatLabel('gedcom') },
    { value: 'gramps_xml', label: formatLabel('gramps_xml') },
    { value: 'json', label: formatLabel('json') }
  ];
  const acceptedExtensions = '.ged,.gramps,.xml,.json';
  const MAX_IMPORT_FILE_BYTES = 100 * 1024 * 1024;

  let fileInput: HTMLInputElement | null = null;
  let selectedFile: File | null = null;
  let selectedFileName = '';
  let selectedFormat: ImportFormat = 'gedcom';
  let detectedFormat: ImportFormat | null = null;
  let formatWasManuallyChanged = false;
  let dragActive = false;

  let busy = false;
  let error = '';
  let info = '';
  let currentStep = 1;

  let jobId = '';
  let jobStatus: ImportJobStatus | null = null;
  let logMessages: string[] = [];
  let entityCounts = normalizeEntityCounts(null);
  let displayedProgress = 0;
  let sseConnected = false;
  let desktopDataDir = '';
  let diagnosticsBusy = false;
  let pollFailureCount = 0;

  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let progressTimer: ReturnType<typeof setInterval> | null = null;
  let eventSource: EventSource | null = null;

  function inTauri(): boolean {
    return typeof window !== 'undefined' && Boolean(window.__TAURI_INTERNALS__);
  }

  async function invokeTauri<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    const { invoke } = await import('@tauri-apps/api/core');
    const tauriInvoke = invoke as TauriInvoke;
    return tauriInvoke<T>(command, args);
  }

  async function saveBlobWithDesktopDialog(blob: Blob, suggestedName: string): Promise<boolean> {
    const destination = await invokeTauri<string | null>('save_file_dialog', {
      title: 'Save diagnostics bundle',
      defaultName: suggestedName
    });

    if (!destination) {
      return false;
    }

    const bytes = Array.from(new Uint8Array(await blob.arrayBuffer()));
    await invokeTauri<void>('write_binary_file', {
      path: destination,
      bytes
    });
    return true;
  }

  function resetTimers(): void {
    if (pollTimer) {
      clearInterval(pollTimer);
      pollTimer = null;
    }

    if (progressTimer) {
      clearInterval(progressTimer);
      progressTimer = null;
    }

    if (eventSource) {
      eventSource.close();
      eventSource = null;
    }

    sseConnected = false;
  }

  function saveWizardState(): void {
    const state = {
      currentStep,
      selectedFileName,
      selectedFormat,
      formatWasManuallyChanged
    };

    try {
      sessionStorage.setItem('import_wizard_state', JSON.stringify(state));
    } catch {
      // Silently ignore if sessionStorage fails
    }
  }

  function restoreWizardState(): void {
    const stored = sessionStorage.getItem('import_wizard_state');
    if (!stored) {
      return;
    }

    try {
      const state = JSON.parse(stored) as {
        currentStep?: number;
        selectedFileName?: string;
        selectedFormat?: ImportFormat;
        formatWasManuallyChanged?: boolean;
      };

      if (typeof state.currentStep === 'number' && state.currentStep > 1) {
        currentStep = state.currentStep;
      }
      if (typeof state.selectedFileName === 'string') {
        selectedFileName = state.selectedFileName;
      }
      if (typeof state.selectedFormat === 'string' && ['gedcom', 'gramps_xml', 'json'].includes(state.selectedFormat)) {
        selectedFormat = state.selectedFormat;
      }
      if (typeof state.formatWasManuallyChanged === 'boolean') {
        formatWasManuallyChanged = state.formatWasManuallyChanged;
      }

      if (currentStep > 1) {
        info = 'Wizard state restored from previous session';
      }
    } catch (e) {
      sessionStorage.removeItem('import_wizard_state');
    }
  }

  function clearWizardState(): void {
    try {
      sessionStorage.removeItem('import_wizard_state');
    } catch {
      // Silently ignore if sessionStorage fails
    }
  }

  function applySelectedFile(file: File): void {
    selectedFile = file;
    selectedFileName = file.name;
    detectedFormat = detectImportFormat(file.name);
    if (!formatWasManuallyChanged && detectedFormat) {
      selectedFormat = detectedFormat;
    }
    error = '';
    info = '';
  }

  async function selectFileFromDesktopDialog(): Promise<void> {
    try {
      if (inTauri()) {
        const targetPath = await invokeTauri<string | null>('open_file_dialog', {
          title: 'Select import file',
          filters: ['ged', 'gramps', 'xml', 'json']
        });

        if (!targetPath) {
          return;
        }

        const bytes = await invokeTauri<number[]>('read_binary_file', { path: targetPath });
        const fileName = targetPath.split(/[\\/]/).pop() ?? 'import.dat';
        applySelectedFile(new File([new Uint8Array(bytes)], fileName));
        info = `Selected ${fileName}`;
        return;
      }

      fileInput?.click();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to select file';
    }
  }

  function handleInputChange(event: Event): void {
    const target = event.currentTarget as HTMLInputElement;
    const file = target.files?.[0];
    if (!file) {
      return;
    }
    applySelectedFile(file);
  }

  function handleDrop(event: DragEvent): void {
    event.preventDefault();
    dragActive = false;

    const file = event.dataTransfer?.files?.[0];
    if (!file) {
      return;
    }

    applySelectedFile(file);
  }

  function appendLogs(nextMessages: string[]): void {
    logMessages = mergeUniqueLogMessages(logMessages, nextMessages);
  }

  function triggerBrowserDownload(blob: Blob, fileName: string): void {
    const url = URL.createObjectURL(blob);

    try {
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = fileName;
      anchor.style.display = 'none';
      document.body.append(anchor);
      anchor.click();
      anchor.remove();
    } finally {
      URL.revokeObjectURL(url);
    }
  }

  function syncJobStatus(nextStatus: ImportJobStatus): void {
    jobStatus = nextStatus;
    appendLogs(nextStatus.log_messages ?? []);
    if (nextStatus.entities_imported_by_type) {
      entityCounts = normalizeEntityCounts(nextStatus.entities_imported_by_type);
    }

    if (nextStatus.status === 'completed') {
      currentStep = 3;
      busy = false;
      displayedProgress = 100;
      resetTimers();
      clearWizardState();
    } else if (nextStatus.status === 'failed') {
      currentStep = 2;
      busy = false;
      displayedProgress = 100;
      error = nextStatus.errors[0] ?? 'Import failed';
      resetTimers();
    }
  }

  async function pollImportStatus(): Promise<void> {
    if (!jobId) {
      return;
    }

    try {
      const nextStatus = await api.get<ImportJobStatus>(`/api/v1/import/${jobId}`);
      pollFailureCount = 0;
      syncJobStatus(nextStatus);
    } catch (err) {
      pollFailureCount += 1;
      const message = err instanceof Error ? err.message : 'Failed to poll import status';
      appendLogs([`Status poll retry ${pollFailureCount}: ${message}`]);

      if (pollFailureCount >= 8) {
        error = message;
        busy = false;
        resetTimers();
      }
    }
  }

  function startProgressAnimation(): void {
    if (progressTimer) {
      clearInterval(progressTimer);
    }

    progressTimer = setInterval(() => {
      const target = jobStatus?.progress_pct ?? 0;
      if (displayedProgress < target) {
        displayedProgress = Math.min(target, displayedProgress + 4);
        return;
      }

      if (busy && displayedProgress < 95) {
        displayedProgress += 1;
      }
    }, 180);
  }

  function subscribeToCompletion(): void {
    if (eventSource) {
      eventSource.close();
    }

    eventSource = new EventSource(api.url('/api/v1/events/stream?types=import.completed'));

    eventSource.onopen = () => {
      sseConnected = true;
    };

    eventSource.onerror = () => {
      sseConnected = false;
    };

    eventSource.addEventListener('import.completed', (event) => {
      const payload = JSON.parse((event as MessageEvent<string>).data) as ImportCompletedEvent;
      if (payload.job_id !== jobId) {
        return;
      }

      appendLogs(['SSE update received: import.completed']);
      entityCounts = normalizeEntityCounts(payload.entities_imported);
      void pollImportStatus();
    });
  }

  async function startImport(): Promise<void> {
    if (!selectedFile) {
      error = 'Select a file to import.';
      return;
    }

    if (selectedFile.size > MAX_IMPORT_FILE_BYTES) {
      const maxMb = Math.floor(MAX_IMPORT_FILE_BYTES / (1024 * 1024));
      const currentMb = (selectedFile.size / (1024 * 1024)).toFixed(1);
      error = `Selected file is ${currentMb} MB. Current import limit is ${maxMb} MB.`;
      appendLogs([`Import blocked: file exceeds ${maxMb} MB limit.`]);
      return;
    }

    busy = true;
    error = '';
    info = '';
    currentStep = 2;
    jobId = '';
    jobStatus = null;
    logMessages = [`Preparing import for ${selectedFile.name}`];
    entityCounts = normalizeEntityCounts(null);
    displayedProgress = 4;
    pollFailureCount = 0;

    try {
      const formData = new FormData();
      formData.set('format', selectedFormat);
      formData.set('file', selectedFile, selectedFile.name);

      const accepted = await api.postFormData<ImportAcceptedResponse>('/api/v1/import', formData);
      jobId = accepted.job_id;
      appendLogs([
        `Import job started: ${accepted.job_id}`,
        `Polling ${accepted.status_url} every 1 second`
      ]);

      subscribeToCompletion();
      startProgressAnimation();
      await pollImportStatus();

      if (pollTimer) {
        clearInterval(pollTimer);
      }

      pollTimer = setInterval(() => {
        void pollImportStatus();
      }, 1000);
    } catch (err) {
      busy = false;
      currentStep = 2;
      resetTimers();
      error = err instanceof Error ? err.message : 'Failed to start import';
      appendLogs([`Import start failed: ${error}`]);
    }
  }

  function resetWizard(): void {
    resetTimers();
    selectedFile = null;
    selectedFileName = '';
    detectedFormat = null;
    selectedFormat = 'gedcom';
    formatWasManuallyChanged = false;
    busy = false;
    error = '';
    info = '';
    currentStep = 1;
    jobId = '';
    jobStatus = null;
    logMessages = [];
    entityCounts = normalizeEntityCounts(null);
    displayedProgress = 0;
  }

  function downloadImportLog(): void {
    if (!jobStatus) {
      return;
    }

    const payload = {
      ...jobStatus,
      entities_imported_by_type: entityCounts,
      log_messages: logMessages
    };

    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
    triggerBrowserDownload(blob, `rustygene-import-${jobStatus.job_id}.json`);
  }

  async function downloadDiagnosticsBundle(): Promise<void> {
    diagnosticsBusy = true;
    error = '';

    try {
      const artifact = await api.download('/api/v1/debug/bundle');
      const fallbackName = `rustygene-diagnostics-${new Date().toISOString().replace(/[:.]/g, '-')}.json`;
      const fileName = artifact.fileName ?? fallbackName;

      if (inTauri()) {
        const saved = await saveBlobWithDesktopDialog(artifact.blob, fileName);
        if (!saved) {
          appendLogs(['Diagnostics bundle save cancelled by user.']);
          return;
        }
      } else {
        triggerBrowserDownload(artifact.blob, fileName);
      }

      appendLogs(['Diagnostics bundle downloaded from /api/v1/debug/bundle']);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to download diagnostics bundle';
      error = message;
      appendLogs([`Diagnostics download failed: ${message}`]);
    } finally {
      diagnosticsBusy = false;
    }
  }

  onDestroy(() => {
    resetTimers();
  });

  onMount(() => {
    restoreWizardState();
    if (inTauri()) {
      void invokeTauri<string>('get_data_dir')
        .then((path) => {
          desktopDataDir = path;
          appendLogs([
            `Desktop diagnostics directory: ${path}`,
            `Desktop runtime log file: ${path}/rustygene-desktop.log`
          ]);
        })
        .catch(() => {
          desktopDataDir = '';
        });
    }
  });

  // Auto-save wizard state when it changes
  $: if (currentStep || selectedFormat) {
    saveWizardState();
  }
</script>

<main class="panel">
  <header class="hero">
    <div>
      <h1>Import wizard</h1>
      <p>Load GEDCOM, Gramps XML, or JSON data, watch progress live, and review the post-import report.</p>
    </div>
    <div class="steps" aria-label="Import progress steps">
      <span class:active={currentStep >= 1}>1. File</span>
      <span class:active={currentStep >= 2}>2. Progress</span>
      <span class:active={currentStep >= 3}>3. Report</span>
    </div>
  </header>

  <section class="card">
    <h2>Step 1 · File selection</h2>

    <button
      type="button"
      class:drag-active={dragActive}
      class="dropzone"
      on:dragenter|preventDefault={() => (dragActive = true)}
      on:dragover|preventDefault={() => (dragActive = true)}
      on:dragleave|preventDefault={() => (dragActive = false)}
      on:drop={handleDrop}
      on:click={selectFileFromDesktopDialog}
    >
      <strong>Drag and drop a file here</strong>
      <span>or click to browse files {#if inTauri()}using the desktop picker{/if}</span>
      <small>Supported formats: GEDCOM 5.5.1 (.ged), Gramps XML (.gramps, .xml), JSON (.json)</small>
    </button>

    <input
      bind:this={fileInput}
      class="hidden-input"
      type="file"
      accept={acceptedExtensions}
      on:change={handleInputChange}
    />

    <div class="grid two-up">
      <label>
        Selected file
        <input type="text" readonly value={selectedFileName || 'No file selected'} />
      </label>

      <label>
        Format
        <select
          bind:value={selectedFormat}
          on:change={() => {
            formatWasManuallyChanged = true;
          }}
        >
          {#each formatOptions as formatOption}
            <option value={formatOption.value}>{formatOption.label}</option>
          {/each}
        </select>
      </label>
    </div>

    {#if detectedFormat}
      <p class="hint">Auto-detected format: <strong>{formatLabel(detectedFormat)}</strong></p>
    {/if}

    <div class="actions">
      <button type="button" on:click={startImport} disabled={busy || !selectedFile}>
        {busy ? 'Import running…' : 'Start Import'}
      </button>
      <button type="button" class="secondary" on:click={() => goto('/import/merge')} disabled={busy}>
        Open merge review
      </button>
      <button type="button" class="secondary" on:click={selectFileFromDesktopDialog} disabled={busy}>
        Browse files
      </button>
    </div>
  </section>

  <section class="card">
    <div class="section-header">
      <h2>Step 2 · Live progress</h2>
      <span class:connected={sseConnected} class="status-pill">
        {sseConnected ? 'SSE connected' : 'SSE waiting'}
      </span>
    </div>

    <div class="progress-shell" aria-live="polite">
      <div class="progress-bar" style={`width: ${Math.min(displayedProgress, 100)}%`}></div>
    </div>
    <p class="progress-meta">
      {#if jobStatus}
        {jobStatus.status} · {displayedProgress}%
      {:else}
        Waiting to start.
      {/if}
    </p>

    <div class="log-panel">
      <div class="section-header">
        <h3>Import log</h3>
        {#if jobId}
          <span class="mono">{jobId}</span>
        {/if}
      </div>

      {#if desktopDataDir}
        <p class="hint">Desktop diagnostics directory: <span class="mono">{desktopDataDir}</span></p>
      {/if}

      <div class="actions">
        <button type="button" class="secondary" on:click={downloadDiagnosticsBundle} disabled={diagnosticsBusy}>
          {diagnosticsBusy ? 'Preparing diagnostics…' : 'Download diagnostics bundle'}
        </button>
      </div>

      {#if logMessages.length === 0}
        <p class="empty">No log entries yet.</p>
      {:else}
        <ol>
          {#each logMessages as message}
            <li>{message}</li>
          {/each}
        </ol>
      {/if}
    </div>
  </section>

  <section class="card">
    <h2>Step 3 · Import report</h2>

    {#if jobStatus?.status === 'completed'}
      <div class="summary-grid">
        {#each reportCards(entityCounts) as card}
          <article class="summary-card">
            <span>{card.label}</span>
            <strong>{card.value}</strong>
          </article>
        {/each}
      </div>

      {#if (jobStatus.warning_details ?? []).length > 0}
        <details class="warnings" open>
          <summary>Warnings ({jobStatus.warning_details?.length ?? 0})</summary>
          {#each jobStatus.warning_details ?? [] as warning (warning.code)}
            <section class="warning-group">
              <h3>{warning.title}</h3>
              <ul>
                {#each Object.entries(warning.counts) as [tag, count]}
                  <li><span class="mono">{tag}</span> <strong>{count}</strong></li>
                {/each}
              </ul>
            </section>
          {/each}
        </details>
      {/if}

      {#if (jobStatus.errors ?? []).length > 0}
        <div class="errors-block">
          <h3>Import errors</h3>
          <ul>
            {#each jobStatus.errors as item}
              <li>{item}</li>
            {/each}
          </ul>
        </div>
      {/if}

      <div class="actions">
        <button type="button" on:click={() => goto('/persons')}>View Persons</button>
        <button type="button" class="secondary" on:click={resetWizard}>Import another file</button>
        <button type="button" class="secondary" on:click={downloadImportLog}>View import log</button>
      </div>
    {:else}
      <p class="empty">Complete an import to see the entity counts and warning report.</p>
    {/if}
  </section>

  {#if info}
    <p class="ok">{info}</p>
  {/if}

  {#if error}
    <p class="error">{error}</p>
  {/if}
</main>

<style>
  .panel {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 0.75rem;
    padding: 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    box-shadow: var(--shadow-sm);
  }

  .hero,
  .section-header,
  .actions,
  .steps,
  .grid {
    display: flex;
  }

  .hero,
  .section-header {
    justify-content: space-between;
    gap: 1rem;
    align-items: center;
  }

  .hero h1,
  .hero p,
  h2,
  h3 {
    margin: 0;
    color: var(--color-text);
  }

  .hero p {
    color: var(--color-muted);
    margin-top: 0.35rem;
  }

  .steps {
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .steps span {
    border-radius: 999px;
    padding: 0.35rem 0.7rem;
    background: var(--color-surface-soft);
    color: var(--color-muted);
    border: 1px solid var(--color-border);
    font-size: 0.88rem;
  }

  .steps span.active {
    background: var(--color-surface);
    color: var(--color-primary);
    border-color: var(--color-primary);
    font-weight: 600;
  }

  .card {
    border: 1px solid var(--color-border);
    border-radius: 0.75rem;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
    background: var(--color-surface);
  }

  .dropzone {
    width: 100%;
    border: 2px dashed var(--color-primary);
    border-radius: 0.75rem;
    background: var(--color-surface-soft);
    color: var(--color-text);
    padding: 1.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    text-align: left;
  }

  .dropzone.drag-active {
    border-color: var(--color-primary);
    background: var(--color-surface);
  }

  .hidden-input {
    display: none;
  }

  .grid {
    gap: 0.75rem;
    flex-wrap: wrap;
  }

  .two-up > * {
    flex: 1 1 18rem;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    color: var(--color-text);
    font-size: 0.95rem;
  }

  input,
  select {
    border: 1px solid var(--color-border);
    border-radius: 0.45rem;
    padding: 0.55rem 0.65rem;
    font: inherit;
    background: var(--color-surface);
    color: var(--color-text);
  }

  .hint,
  .progress-meta,
  .empty,
  small {
    color: var(--color-muted);
    margin: 0;
  }

  .actions {
    gap: 0.75rem;
    flex-wrap: wrap;
  }

  button {
    border: 0;
    border-radius: 0.45rem;
    padding: 0.55rem 0.85rem;
    background: var(--color-primary);
    color: #ffffff;
    cursor: pointer;
    width: fit-content;
  }

  button.secondary {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    color: var(--color-text);
  }

  button:disabled {
    opacity: 0.65;
    cursor: not-allowed;
  }

  .status-pill {
    border-radius: 999px;
    padding: 0.35rem 0.7rem;
    background: var(--color-surface-soft);
    color: var(--color-muted);
    font-size: 0.85rem;
    border: 1px solid var(--color-border);
  }

  .status-pill.connected {
    color: var(--color-primary);
    border-color: var(--color-primary);
  }

  .progress-shell {
    width: 100%;
    height: 0.9rem;
    background: var(--color-border);
    border-radius: 999px;
    overflow: hidden;
  }

  .progress-bar {
    height: 100%;
    background: var(--color-primary);
    transition: width 0.18s ease;
  }

  .log-panel {
    border: 1px solid var(--color-border);
    border-radius: 0.65rem;
    background: var(--color-surface-soft);
    padding: 0.85rem;
    max-height: 18rem;
    overflow: auto;
  }

  .log-panel ol {
    margin: 0.5rem 0 0;
    padding-left: 1.1rem;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    color: var(--color-text);
  }

  .mono {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.84rem;
  }

  .summary-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(11rem, 1fr));
    gap: 0.75rem;
  }

  .summary-card {
    border: 1px solid var(--color-border);
    border-radius: 0.75rem;
    padding: 0.85rem;
    background: var(--color-surface);
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .summary-card span {
    color: var(--color-muted);
  }

  .summary-card strong {
    font-size: 1.6rem;
    color: var(--color-primary);
  }

  .warnings,
  .errors-block {
    border: 1px solid var(--color-border);
    border-radius: 0.65rem;
    padding: 0.85rem;
    background: var(--color-surface);
  }

  .errors-block {
    border-color: var(--color-danger);
  }

  .warning-group + .warning-group {
    margin-top: 0.85rem;
  }

  .warning-group ul,
  .errors-block ul {
    margin: 0.5rem 0 0;
    padding-left: 1.1rem;
    color: var(--color-text);
  }

  .warning-group li {
    display: flex;
    gap: 0.5rem;
    align-items: baseline;
    margin-bottom: 0.25rem;
  }

  .ok,
  .error {
    margin: 0;
  }

  .ok {
    color: var(--color-primary);
  }

  .error {
    color: var(--color-danger);
  }
</style>
