<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';

  type NoteType = 'General' | 'Research' | 'Transcript' | 'SourceText' | 'Todo' | string;

  type NoteItem = {
    id: string;
    text: string;
    note_type: NoteType;
    linked_entity_id?: string | null;
    linked_entity_type?: string | null;
    position_x_pct?: number | null;
    position_y_pct?: number | null;
  };

  export let entityId: string;
  export let entityType: 'person' | 'event' | 'source' | 'repository' | 'family' | 'media' = 'person';

  let notes: NoteItem[] = [];
  let loading = false;
  let error = '';
  let showForm = false;
  let saving = false;
  let editingId = '';

  let noteText = '';
  let noteType: NoteType = 'General';

  const noteTypeOptions: NoteType[] = ['General', 'Research', 'Transcript', 'SourceText', 'Todo'];

  function renderSafeText(text: string): string {
    return text
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/\n/g, '<br/>');
  }

  function badgeLabel(type: NoteType): string {
    if (type === 'SourceText') {
      return 'Source';
    }
    return String(type);
  }

  async function loadNotes(): Promise<void> {
    loading = true;
    error = '';
    try {
      const query = new URLSearchParams({ entity_id: entityId });
      notes = await api.get<NoteItem[]>(`/api/v1/notes?${query.toString()}`);
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load notes';
    } finally {
      loading = false;
    }
  }

  async function saveNote(): Promise<void> {
    const text = noteText.trim();
    if (!text) {
      error = 'Note text is required.';
      return;
    }

    saving = true;
    error = '';
    const payload = {
      text,
      note_type: noteType,
      linked_entity_id: entityId,
      linked_entity_type: entityType
    };

    try {
      if (editingId) {
        await api.put(`/api/v1/notes/${editingId}`, payload);
      } else {
        await api.post('/api/v1/notes', payload);
      }

      noteText = '';
      noteType = 'General';
      editingId = '';
      showForm = false;
      await loadNotes();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to save note';
    } finally {
      saving = false;
    }
  }

  function editNote(note: NoteItem): void {
    editingId = note.id;
    noteText = note.text;
    noteType = note.note_type;
    showForm = true;
  }

  async function deleteNote(noteId: string): Promise<void> {
    const confirmed = confirm('Delete this note?');
    if (!confirmed) {
      return;
    }

    error = '';
    try {
      await api.del(`/api/v1/notes/${noteId}`);
      await loadNotes();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to delete note';
    }
  }

  onMount(async () => {
    await loadNotes();
  });
</script>

<section class="notes">
  <div class="head">
    <h3>Notes</h3>
    <button type="button" on:click={() => (showForm = !showForm)}>
      {showForm ? 'Close note form' : 'Add note'}
    </button>
  </div>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if showForm}
    <div class="form">
      <label>
        Note type
        <select bind:value={noteType}>
          {#each noteTypeOptions as option}
            <option value={option}>{option}</option>
          {/each}
        </select>
      </label>
      <label>
        Text
        <textarea rows="4" bind:value={noteText}></textarea>
      </label>
      <div class="actions">
        <button type="button" class="secondary" on:click={() => {
          showForm = false;
          editingId = '';
          noteText = '';
          noteType = 'General';
        }}>Cancel</button>
        <button type="button" on:click={saveNote} disabled={saving}>{saving ? 'Saving…' : 'Save note'}</button>
      </div>
    </div>
  {/if}

  {#if loading}
    <p>Loading notes…</p>
  {:else if notes.length === 0}
    <p>No notes linked.</p>
  {:else}
    <div class="list">
      {#each notes as note}
        <article class="note-card">
          <div class="meta">
            <span class="badge">{badgeLabel(note.note_type)}</span>
            <code>{note.id}</code>
          </div>
          <p class="body">{@html renderSafeText(note.text)}</p>
          <p class="linked">
            Linked: {note.linked_entity_type ?? entityType} · {note.linked_entity_id ?? entityId}
          </p>
          <div class="actions">
            <button type="button" class="secondary" on:click={() => editNote(note)}>Edit</button>
            <button type="button" class="danger" on:click={() => deleteNote(note.id)}>Delete</button>
          </div>
        </article>
      {/each}
    </div>
  {/if}
</section>

<style>
  .notes {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
  }

  .head h3 {
    margin: 0;
  }

  .form {
    border: 1px solid #cbd5e1;
    border-radius: 0.6rem;
    padding: 0.65rem;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.9rem;
  }

  textarea,
  select {
    border: 1px solid var(--color-border);
    border-radius: 0.45rem;
    padding: 0.4rem 0.5rem;
    font: inherit;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .note-card {
    border: 1px solid var(--color-border);
    border-radius: 0.55rem;
    padding: 0.6rem;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    background: var(--color-surface);
  }

  .meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.4rem;
  }

  .badge {
    background: var(--color-surface-soft);
    color: var(--color-primary);
    border-radius: 999px;
    padding: 0.1rem 0.5rem;
    font-size: 0.75rem;
    font-weight: 700;
    text-transform: uppercase;
  }

  .body,
  .linked {
    margin: 0;
    color: var(--color-text);
  }

  .linked {
    font-size: 0.82rem;
    color: var(--color-muted);
  }

  .actions {
    display: flex;
    gap: 0.4rem;
  }

  button {
    border: 0;
    border-radius: 0.4rem;
    background: var(--color-primary);
    color: var(--color-surface);
    padding: 0.35rem 0.55rem;
    cursor: pointer;
    width: fit-content;
  }

  .secondary {
    background: var(--color-muted);
  }

  .danger {
    background: var(--color-danger);
  }

  .error {
    margin: 0;
    color: var(--color-danger);
  }

  code {
    background: var(--color-surface-soft);
    border-radius: 0.3rem;
    padding: 0.05rem 0.3rem;
  }
</style>
