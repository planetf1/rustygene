<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { api } from '$lib/api';

  type StagingProposal = {
    id: string;
    entity_type: string;
    entity_id: string;
    proposed_field: string;
    proposed_value: unknown;
    current_value: unknown;
    diff_summary: string;
    confidence: number;
    source: string | null;
    status: string;
    created_at: string;
    reviewed_at: string | null;
    reviewed_by: string | null;
    review_note: string | null;
  };

  let loading = false;
  let error = '';
  let proposals: StagingProposal[] = [];

  let statusFilter = 'pending';
  let entityFilter = 'all';
  let search = '';
  let selected = new Set<string>();
  let reviewer = 'ui';
  let actionBusy = false;

  const statusOptions = ['pending', 'approved', 'rejected', 'disputed', 'all'];

  async function loadProposals(): Promise<void> {
    loading = true;
    error = '';

    try {
      const query = statusFilter === 'all' ? '' : `?status=${encodeURIComponent(statusFilter)}`;
      proposals = await api.get<StagingProposal[]>(`/api/v1/staging${query}`);
      selected = new Set();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load staging proposals';
    } finally {
      loading = false;
    }
  }

  function asText(value: unknown): string {
    if (value === null || value === undefined) {
      return '—';
    }

    if (typeof value === 'string') {
      return value;
    }

    try {
      return JSON.stringify(value, null, 2);
    } catch {
      return String(value);
    }
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

  function selectVisible(): void {
    selected = new Set(filtered.map((proposal) => proposal.id));
  }

  function clearSelection(): void {
    selected = new Set();
  }

  async function reviewSingle(id: string, action: 'approve' | 'reject'): Promise<void> {
    actionBusy = true;
    error = '';

    try {
      if (action === 'approve') {
        await api.post(`/api/v1/staging/${id}/approve`, { reviewer });
      } else {
        const reason = prompt('Rejection reason:')?.trim();
        if (!reason) {
          return;
        }

        await api.post(`/api/v1/staging/${id}/reject`, { reviewer, reason });
      }

      await loadProposals();
    } catch (err) {
      error = err instanceof Error ? err.message : `Failed to ${action} proposal`;
    } finally {
      actionBusy = false;
    }
  }

  async function bulkReview(action: 'approve' | 'reject'): Promise<void> {
    const ids = Array.from(selected);
    if (ids.length === 0) {
      return;
    }

    actionBusy = true;
    error = '';

    try {
      const payload: { ids: string[]; action: 'approve' | 'reject'; reviewer: string; reason?: string } = {
        ids,
        action,
        reviewer
      };

      if (action === 'reject') {
        const reason = prompt('Bulk rejection reason:')?.trim();
        if (!reason) {
          return;
        }

        payload.reason = reason;
      }

      await api.post('/api/v1/staging/bulk', payload);
      await loadProposals();
    } catch (err) {
      error = err instanceof Error ? err.message : `Failed to ${action} selected proposals`;
    } finally {
      actionBusy = false;
    }
  }

  function navigateToEntity(proposal: StagingProposal): void {
    const path = `/${proposal.entity_type}s/${proposal.entity_id}`;
    void goto(path);
  }

  function isPending(proposal: StagingProposal): boolean {
    return proposal.status.toLowerCase() === 'pending';
  }

  $: filtered = proposals
    .filter((proposal) => entityFilter === 'all' || proposal.entity_type === entityFilter)
    .filter((proposal) => {
      if (!search.trim()) {
        return true;
      }

      const needle = search.toLowerCase();
      return (
        proposal.diff_summary.toLowerCase().includes(needle) ||
        proposal.entity_id.toLowerCase().includes(needle) ||
        proposal.proposed_field.toLowerCase().includes(needle)
      );
    });

  $: entityTypes = ['all', ...Array.from(new Set(proposals.map((proposal) => proposal.entity_type))).sort()];

  onMount(async () => {
    await loadProposals();
  });
</script>

<main class="panel">
  <header class="header">
    <div>
      <h1>Review queue</h1>
      <p>Proposal triage for staged assertions.</p>
    </div>
    <div class="controls compact">
      <label>
        Reviewer
        <input type="text" bind:value={reviewer} />
      </label>
    </div>
  </header>

  <section class="controls">
    <label>
      Status
      <select bind:value={statusFilter} on:change={loadProposals}>
        {#each statusOptions as value}
          <option value={value}>{value}</option>
        {/each}
      </select>
    </label>

    <label>
      Entity
      <select bind:value={entityFilter}>
        {#each entityTypes as value}
          <option value={value}>{value}</option>
        {/each}
      </select>
    </label>

    <label class="search">
      Search
      <input type="search" bind:value={search} placeholder="field, entity ID, summary…" />
    </label>
  </section>

  <section class="bulk-actions">
    <button type="button" class="secondary" on:click={selectVisible}>Select visible</button>
    <button type="button" class="secondary" on:click={clearSelection}>Clear selection</button>
    <button type="button" on:click={() => bulkReview('approve')} disabled={selected.size === 0 || actionBusy}>
      Bulk approve ({selected.size})
    </button>
    <button
      type="button"
      class="danger"
      on:click={() => bulkReview('reject')}
      disabled={selected.size === 0 || actionBusy}
    >
      Bulk reject ({selected.size})
    </button>
  </section>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if loading}
    <p>Loading proposals…</p>
  {:else if filtered.length === 0}
    <p>No proposals match current filters.</p>
  {:else}
    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th></th>
            <th>Entity</th>
            <th>Field</th>
            <th>Diff summary</th>
            <th>Current</th>
            <th>Proposed</th>
            <th>Confidence</th>
            <th>Status</th>
            <th>Actions</th>
          </tr>
        </thead>
        <tbody>
          {#each filtered as proposal}
            <tr>
              <td>
                <input
                  type="checkbox"
                  checked={selected.has(proposal.id)}
                  on:change={() => toggleSelection(proposal.id)}
                />
              </td>
              <td>
                <button class="linkish" type="button" on:click={() => navigateToEntity(proposal)}>
                  {proposal.entity_type} · {proposal.entity_id}
                </button>
              </td>
              <td>{proposal.proposed_field}</td>
              <td>{proposal.diff_summary}</td>
              <td><pre>{asText(proposal.current_value)}</pre></td>
              <td><pre>{asText(proposal.proposed_value)}</pre></td>
              <td>{proposal.confidence.toFixed(2)}</td>
              <td>
                <span class={`status ${proposal.status.toLowerCase()}`}>{proposal.status}</span>
              </td>
              <td class="actions-cell">
                <button
                  type="button"
                  disabled={!isPending(proposal) || actionBusy}
                  on:click={() => reviewSingle(proposal.id, 'approve')}
                >Approve</button>
                <button
                  type="button"
                  class="danger"
                  disabled={!isPending(proposal) || actionBusy}
                  on:click={() => reviewSingle(proposal.id, 'reject')}
                >Reject</button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</main>

<style>
  .panel {
    background: #fff;
    border: 1px solid #e2e8f0;
    border-radius: 0.75rem;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
  }

  .header {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: flex-end;
  }

  .header h1 {
    margin: 0;
  }

  .header p {
    margin: 0.25rem 0 0;
    color: #64748b;
  }

  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    align-items: end;
  }

  .controls.compact {
    align-items: center;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.9rem;
    color: #334155;
  }

  .search {
    min-width: 22rem;
    flex: 1;
  }

  input,
  select {
    border: 1px solid #cbd5e1;
    border-radius: 0.4rem;
    padding: 0.35rem 0.45rem;
    font: inherit;
  }

  .bulk-actions {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  button {
    border: 0;
    border-radius: 0.45rem;
    padding: 0.4rem 0.65rem;
    background: #2563eb;
    color: #fff;
    cursor: pointer;
  }

  button.secondary {
    background: #475569;
  }

  button.danger {
    background: #b91c1c;
  }

  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .table-wrap {
    overflow-x: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9rem;
  }

  th,
  td {
    border-bottom: 1px solid #e2e8f0;
    text-align: left;
    vertical-align: top;
    padding: 0.45rem;
  }

  th {
    color: #475569;
    font-size: 0.82rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  pre {
    margin: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-width: 22rem;
    font-size: 0.8rem;
    color: #0f172a;
  }

  .status {
    border-radius: 999px;
    padding: 0.1rem 0.45rem;
    font-size: 0.75rem;
    text-transform: uppercase;
    font-weight: 700;
    background: #e2e8f0;
    color: #334155;
  }

  .status.pending {
    background: #fef3c7;
    color: #92400e;
  }

  .status.approved {
    background: #dcfce7;
    color: #166534;
  }

  .status.rejected {
    background: #fee2e2;
    color: #991b1b;
  }

  .status.disputed {
    background: #fce7f3;
    color: #9d174d;
  }

  .actions-cell {
    display: flex;
    gap: 0.4rem;
  }

  .error {
    margin: 0;
    color: #b91c1c;
  }

  .linkish {
    border: 0;
    background: transparent;
    color: #1d4ed8;
    cursor: pointer;
    padding: 0;
    font: inherit;
    text-align: left;
  }
</style>
