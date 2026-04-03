<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';

  type MediaItem = {
    id: string;
    file_path: string;
    content_hash: string;
    mime_type: string;
    thumbnail_url: string;
    caption: string | null;
    tags: string[];
    albums: string[];
    link_count: number;
  };

  type AlbumItem = {
    id: string;
    name: string;
  };

  type PersonOption = {
    id: string;
    display_name: string;
  };

  type ViewMode = 'grid' | 'list';
  type MediaTypeFilter = 'all' | 'image' | 'document' | 'audio' | 'video';

  let viewMode: ViewMode = 'grid';
  let mediaType: MediaTypeFilter = 'all';
  let selectedEntityId = '';
  let selectedAlbum = '';
  let showUnlinked = false;
  let showUntagged = false;

  let loading = false;
  let uploading = false;
  let error = '';
  let message = '';

  let items: MediaItem[] = [];
  let albums: AlbumItem[] = [];
  let persons: PersonOption[] = [];
  let selected = new Set<string>();

  function mediaFileName(item: MediaItem): string {
    const parts = item.file_path.split('/');
    return parts[parts.length - 1] ?? item.id;
  }

  function isImage(item: MediaItem): boolean {
    return item.mime_type.startsWith('image/');
  }

  function toggleSelection(id: string): void {
    const next = new Set(selected);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    selected = next;
  }

  function clearSelection(): void {
    selected = new Set();
  }

  async function loadGallery(): Promise<void> {
    loading = true;
    error = '';

    try {
      const params = new URLSearchParams();
      if (mediaType !== 'all') {
        params.set('type', mediaType);
      }
      if (selectedEntityId) {
        params.set('entity_id', selectedEntityId);
      }
      if (selectedAlbum) {
        params.set('album', selectedAlbum);
      }
      if (showUnlinked) {
        params.set('unlinked', 'true');
      }
      if (showUntagged) {
        params.set('untagged', 'true');
      }

      const query = params.toString();
      items = await api.get<MediaItem[]>(`/api/v1/media${query ? `?${query}` : ''}`);
      selected = new Set();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load media gallery';
    } finally {
      loading = false;
    }
  }

  async function loadSupportingData(): Promise<void> {
    try {
      const [albumRows, personRows] = await Promise.all([
        api.get<AlbumItem[]>('/api/v1/media/albums'),
        api.get<PersonOption[]>('/api/v1/persons?limit=200&offset=0')
      ]);
      albums = albumRows;
      persons = personRows;
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load media filters';
    }
  }

  async function uploadFiles(files: FileList | File[]): Promise<void> {
    uploading = true;
    error = '';
    message = '';

    try {
      const list = Array.from(files);
      for (const file of list) {
        const form = new FormData();
        form.append('file', file);
        const response = await fetch(api.url('/api/v1/media'), {
          method: 'POST',
          body: form
        });
        if (!response.ok) {
          throw new Error(`Upload failed for ${file.name}`);
        }
      }

      message = `Uploaded ${list.length} file(s).`;
      await loadGallery();
      await loadSupportingData();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Upload failed';
    } finally {
      uploading = false;
    }
  }

  async function addTag(item: MediaItem): Promise<void> {
    const tag = prompt('Tag name:')?.trim() ?? '';
    if (!tag) {
      return;
    }

    error = '';
    try {
      await api.post(`/api/v1/media/${item.id}/tags`, { tag });
      await loadGallery();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to add tag';
    }
  }

  async function removeTag(item: MediaItem, tag: string): Promise<void> {
    error = '';
    try {
      await api.del(`/api/v1/media/${item.id}/tags/${encodeURIComponent(tag)}`);
      await loadGallery();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to remove tag';
    }
  }

  async function createAlbum(): Promise<void> {
    const name = prompt('Album name:')?.trim() ?? '';
    if (!name) {
      return;
    }

    error = '';
    try {
      await api.post('/api/v1/media/albums', { name });
      await loadSupportingData();
      selectedAlbum = name;
      await loadGallery();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to create album';
    }
  }

  async function addSelectedToAlbum(): Promise<void> {
    const ids = Array.from(selected);
    if (!selectedAlbum || ids.length === 0) {
      return;
    }

    error = '';
    try {
      await api.post(`/api/v1/media/albums/${encodeURIComponent(selectedAlbum)}/items`, {
        media_ids: ids
      });
      message = `Added ${ids.length} item(s) to album '${selectedAlbum}'.`;
      clearSelection();
      await loadSupportingData();
      await loadGallery();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to add items to album';
    }
  }

  function onDrop(event: DragEvent): void {
    event.preventDefault();
    const files = event.dataTransfer?.files;
    if (!files || files.length === 0) {
      return;
    }
    void uploadFiles(files);
  }

  function onFileInput(event: Event): void {
    const input = event.currentTarget as HTMLInputElement;
    if (!input.files || input.files.length === 0) {
      return;
    }
    void uploadFiles(input.files);
    input.value = '';
  }

  onMount(async () => {
    await Promise.all([loadSupportingData(), loadGallery()]);
  });
</script>

<main class="panel">
  <header>
    <h1>Media Gallery</h1>
    <p>Browse, upload, and organize media files.</p>
  </header>

  <section class="toolbar">
    <div class="left">
      <button type="button" class:active={viewMode === 'grid'} on:click={() => (viewMode = 'grid')}>Grid</button>
      <button type="button" class:active={viewMode === 'list'} on:click={() => (viewMode = 'list')}>List</button>
      <button type="button" on:click={createAlbum}>Create album</button>
    </div>

    <label class="upload">
      <input type="file" multiple on:change={onFileInput} />
      <span>{uploading ? 'Uploading…' : 'Upload files'}</span>
    </label>
  </section>

  <section class="filters">
    <label>
      File type
      <select bind:value={mediaType} on:change={() => void loadGallery()}>
        <option value="all">All</option>
        <option value="image">Image</option>
        <option value="document">Document</option>
        <option value="audio">Audio</option>
        <option value="video">Video</option>
      </select>
    </label>

    <label>
      Linked entity (person)
      <select bind:value={selectedEntityId} on:change={() => void loadGallery()}>
        <option value="">All</option>
        {#each persons as person}
          <option value={person.id}>{person.display_name}</option>
        {/each}
      </select>
    </label>

    <label>
      Album
      <select bind:value={selectedAlbum} on:change={() => void loadGallery()}>
        <option value="">All</option>
        {#each albums as album}
          <option value={album.name}>{album.name}</option>
        {/each}
      </select>
    </label>

    <label class="check">
      <input type="checkbox" bind:checked={showUnlinked} on:change={() => void loadGallery()} />
      <span>Unlinked only</span>
    </label>

    <label class="check">
      <input type="checkbox" bind:checked={showUntagged} on:change={() => void loadGallery()} />
      <span>Untagged only</span>
    </label>
  </section>

  <section
    class="dropzone"
    role="button"
    tabindex="0"
    on:dragover|preventDefault
    on:drop={onDrop}
    on:keydown={(event) => {
      if (event.key === 'Enter' || event.key === ' ') {
        event.preventDefault();
      }
    }}
  >
    Drop files here to upload.
  </section>

  <section class="album-actions">
    <span>{selected.size} selected</span>
    <button type="button" on:click={addSelectedToAlbum} disabled={selected.size === 0 || !selectedAlbum}>
      Add selected to album
    </button>
    <button type="button" on:click={clearSelection} disabled={selected.size === 0}>Clear selection</button>
  </section>

  {#if message}
    <p class="ok">{message}</p>
  {/if}
  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if loading}
    <p>Loading media…</p>
  {:else if items.length === 0}
    <p>No media files match current filters.</p>
  {:else if viewMode === 'grid'}
    <section class="grid">
      {#each items as item}
        <article class="card">
          <label class="select">
            <input type="checkbox" checked={selected.has(item.id)} on:change={() => toggleSelection(item.id)} />
          </label>

          {#if isImage(item)}
            <img src={item.thumbnail_url} alt={item.caption ?? mediaFileName(item)} loading="lazy" />
          {:else}
            <div class="icon">{item.mime_type}</div>
          {/if}

          <h3>{item.caption ?? mediaFileName(item)}</h3>
          <p>{item.link_count} link(s)</p>

          <div class="pills">
            {#each item.albums as album}
              <span class="album">{album}</span>
            {/each}
          </div>

          <div class="pills">
            {#each item.tags as tag}
              <button type="button" class="tag" on:click={() => void removeTag(item, tag)}>{tag} ×</button>
            {/each}
          </div>

          <button type="button" class="secondary" on:click={() => void addTag(item)}>Add tag</button>
        </article>
      {/each}
    </section>
  {:else}
    <section class="list-wrap">
      <table>
        <thead>
          <tr>
            <th></th>
            <th>Name</th>
            <th>Type</th>
            <th>Links</th>
            <th>Albums</th>
            <th>Tags</th>
            <th>Actions</th>
          </tr>
        </thead>
        <tbody>
          {#each items as item}
            <tr>
              <td><input type="checkbox" checked={selected.has(item.id)} on:change={() => toggleSelection(item.id)} /></td>
              <td>{item.caption ?? mediaFileName(item)}</td>
              <td>{item.mime_type}</td>
              <td>{item.link_count}</td>
              <td>{item.albums.join(', ') || '—'}</td>
              <td>{item.tags.join(', ') || '—'}</td>
              <td><button type="button" class="secondary" on:click={() => void addTag(item)}>Add tag</button></td>
            </tr>
          {/each}
        </tbody>
      </table>
    </section>
  {/if}
</main>

<style>
  .panel {
    background: #ffffff;
    border: 1px solid #e2e8f0;
    border-radius: 0.75rem;
    padding: 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }

  header h1 {
    margin: 0;
  }

  header p {
    margin: 0.25rem 0 0;
    color: #64748b;
  }

  .toolbar,
  .filters,
  .album-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem;
    align-items: center;
  }

  .toolbar {
    justify-content: space-between;
  }

  .toolbar .left {
    display: inline-flex;
    gap: 0.5rem;
  }

  label {
    display: inline-flex;
    flex-direction: column;
    gap: 0.2rem;
    color: #334155;
    font-size: 0.9rem;
  }

  label.check {
    flex-direction: row;
    align-items: center;
    gap: 0.45rem;
  }

  select {
    min-width: 12rem;
    border: 1px solid #cbd5e1;
    border-radius: 0.4rem;
    padding: 0.35rem 0.45rem;
    font: inherit;
  }

  .upload input {
    display: none;
  }

  .upload span,
  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 0.45rem;
    padding: 0.45rem 0.7rem;
    color: #fff;
    background: #2563eb;
    cursor: pointer;
    font: inherit;
  }

  button.secondary {
    background: #475569;
  }

  button.active {
    background: #1d4ed8;
  }

  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .dropzone {
    border: 2px dashed #94a3b8;
    border-radius: 0.7rem;
    padding: 0.9rem;
    text-align: center;
    color: #475569;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 0.75rem;
  }

  .card {
    border: 1px solid #e2e8f0;
    border-radius: 0.65rem;
    padding: 0.6rem;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
  }

  .select {
    flex-direction: row;
    justify-content: flex-end;
  }

  .card img {
    width: 100%;
    aspect-ratio: 1 / 1;
    object-fit: cover;
    border-radius: 0.5rem;
    border: 1px solid #e2e8f0;
    background: #f8fafc;
  }

  .icon {
    width: 100%;
    aspect-ratio: 1 / 1;
    border-radius: 0.5rem;
    border: 1px dashed #cbd5e1;
    display: grid;
    place-items: center;
    color: #475569;
    background: #f8fafc;
    font-size: 0.8rem;
    text-align: center;
    padding: 0.5rem;
  }

  .card h3,
  .card p {
    margin: 0;
  }

  .pills {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }

  .album,
  .tag {
    background: #e2e8f0;
    color: #1e293b;
    border-radius: 999px;
    padding: 0.15rem 0.5rem;
    font-size: 0.75rem;
    border: 0;
  }

  .tag {
    cursor: pointer;
  }

  .list-wrap {
    overflow-x: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  th,
  td {
    border-bottom: 1px solid #e2e8f0;
    padding: 0.45rem;
    text-align: left;
  }

  .ok {
    margin: 0;
    color: #166534;
  }

  .error {
    margin: 0;
    color: #b91c1c;
  }
</style>
