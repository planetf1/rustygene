<script lang="ts">
  import { goto } from '$app/navigation';
  import { onDestroy, onMount } from 'svelte';
  import * as d3 from 'd3';

  import { api } from '$lib/api';
  import { ancestorDataStore, type AncestorApiNode } from '$lib/charts/ancestorStore';

  type PersonListRow = {
    id: string;
    display_name: string;
  };

  type SearchResult = {
    entity_type: string;
    entity_id: string;
    display_name: string;
    snippet: string | null;
  };

  type SearchResponse = {
    results: SearchResult[];
  };

  type RootCrumb = {
    personId: string;
    displayName: string;
  };

  type ArcDatum = {
    key: string;
    personId: string | null;
    displayName: string;
    birthYear: number | null;
    deathYear: number | null;
    confidence: number;
    generation: number;
    slot: number;
    innerRadius: number;
    outerRadius: number;
    startAngle: number;
    endAngle: number;
    isPlaceholder: boolean;
    genderHint: 'male' | 'female' | 'unknown';
  };

  const ringWidth = 70;
  const rootRadius = 48;

  let loading = false;
  let error = '';

  let rootPersonId = '';
  let rootPersonName = '';
  let generations = 5;

  let treeData: AncestorApiNode | null = null;
  let arcs: ArcDatum[] = [];

  let breadcrumbs: RootCrumb[] = [];

  let chartContainer: HTMLDivElement | null = null;
  let tooltipOpen = false;
  let tooltipX = 0;
  let tooltipY = 0;
  let tooltipNode: ArcDatum | null = null;
  let hideTooltipTimer: ReturnType<typeof setTimeout> | null = null;

  let searchInput = '';
  let searchResults: SearchResult[] = [];
  let searchDebounce: ReturnType<typeof setTimeout> | null = null;

  let zoomScale = 1;

  function fullLifeLabel(node: Pick<ArcDatum, 'birthYear' | 'deathYear'>): string {
    const from = node.birthYear === null ? '?' : String(node.birthYear);
    const to = node.deathYear === null ? '?' : String(node.deathYear);
    return `${from} - ${to}`;
  }

  function confidenceOpacity(confidence: number, isPlaceholder: boolean): number {
    if (isPlaceholder) {
      return 0.25;
    }
    return Math.max(0.25, Math.min(1, 0.3 + confidence * 0.7));
  }

  function fillFor(genderHint: ArcDatum['genderHint'], isPlaceholder: boolean): string {
    if (isPlaceholder) {
      return '#cbd5e1';
    }

    if (genderHint === 'male') {
      return '#60a5fa';
    }
    if (genderHint === 'female') {
      return '#f9a8d4';
    }
    return '#94a3b8';
  }

  function strokeFor(isPlaceholder: boolean): string {
    return isPlaceholder ? '#64748b' : '#0f172a';
  }

  function arcPath(arc: ArcDatum): string {
    return (
      d3
        .arc<ArcDatum>()
        .innerRadius(arc.innerRadius)
        .outerRadius(arc.outerRadius)
        .startAngle(arc.startAngle)
        .endAngle(arc.endAngle)(arc) ?? ''
    );
  }

  function centroid(arc: ArcDatum, radialBias = 0.56): { x: number; y: number; angle: number } {
    const angle = (arc.startAngle + arc.endAngle) / 2;
    const radius = arc.innerRadius + (arc.outerRadius - arc.innerRadius) * radialBias;
    return {
      x: Math.cos(angle) * radius,
      y: Math.sin(angle) * radius,
      angle
    };
  }

  function surnameAndGiven(name: string): { surname: string; given: string } {
    const chunks = name.trim().split(/\s+/).filter(Boolean);
    if (chunks.length === 0) {
      return { surname: '?', given: '' };
    }
    if (chunks.length === 1) {
      return { surname: chunks[0], given: '' };
    }
    const surname = chunks[chunks.length - 1];
    const given = chunks.slice(0, chunks.length - 1).join(' ');
    return { surname, given };
  }

  function truncateGiven(given: string): string {
    if (given.length <= 3) {
      return given;
    }
    return `${given.slice(0, 3)}.`;
  }

  function labelRotation(arc: ArcDatum): number {
    const angleDeg = (((arc.startAngle + arc.endAngle) / 2) * 180) / Math.PI;
    const tangent = angleDeg + 90;
    return tangent > 90 && tangent < 270 ? tangent + 180 : tangent;
  }

  function labelAnchor(arc: ArcDatum): 'start' | 'end' {
    const angleDeg = (((arc.startAngle + arc.endAngle) / 2) * 180) / Math.PI;
    return angleDeg > 0 ? 'start' : 'end';
  }

  function canRenderArcLabel(arc: ArcDatum): boolean {
    return arc.endAngle - arc.startAngle > 0.12;
  }

  function nodeAtPath(root: AncestorApiNode | null, pathBits: number[]): AncestorApiNode | null {
    let cursor = root;
    for (const bit of pathBits) {
      if (!cursor) {
        return null;
      }
      cursor = bit === 0 ? cursor.father : cursor.mother;
    }
    return cursor;
  }

  function pathBitsFor(generation: number, slot: number): number[] {
    const bits: number[] = [];
    for (let i = generation - 1; i >= 0; i -= 1) {
      bits.push((slot >> i) & 1);
    }
    return bits;
  }

  function genderHintFromBits(bits: number[]): ArcDatum['genderHint'] {
    if (bits.length === 0) {
      return 'unknown';
    }
    return bits[bits.length - 1] === 0 ? 'male' : 'female';
  }

  function buildArcs(root: AncestorApiNode, maxGenerations: number): ArcDatum[] {
    const result: ArcDatum[] = [];

    for (let generation = 1; generation <= maxGenerations; generation += 1) {
      const slots = 2 ** generation;
      const span = Math.PI / slots;
      const innerRadius = rootRadius + ringWidth * (generation - 1) + 8;
      const outerRadius = innerRadius + ringWidth - 12;

      for (let slot = 0; slot < slots; slot += 1) {
        const startAngle = -Math.PI / 2 + slot * span;
        const endAngle = startAngle + span;
        const bits = pathBitsFor(generation, slot);
        const node = nodeAtPath(root, bits);

        result.push({
          key: `${generation}:${slot}`,
          personId: node?.person_id ?? null,
          displayName: node?.display_name ?? '?',
          birthYear: node?.birth_year ?? null,
          deathYear: node?.death_year ?? null,
          confidence: node?.confidence ?? 0.3,
          generation,
          slot,
          innerRadius,
          outerRadius,
          startAngle,
          endAngle,
          isPlaceholder: node === null,
          genderHint: node ? genderHintFromBits(bits) : 'unknown'
        });
      }
    }

    return result;
  }

  async function fetchPeopleSearch(): Promise<void> {
    const query = searchInput.trim();
    if (query.length < 2) {
      searchResults = [];
      return;
    }

    try {
      const payload = await api.get<SearchResponse>(
        `/api/v1/search?q=${encodeURIComponent(query)}&entity_type=person&limit=8`
      );
      searchResults = payload.results.filter((row) => row.entity_type === 'person');
    } catch {
      searchResults = [];
    }
  }

  function onSearchInput(): void {
    if (searchDebounce) {
      clearTimeout(searchDebounce);
    }

    searchDebounce = setTimeout(() => {
      void fetchPeopleSearch();
    }, 220);
  }

  async function ensureDefaultRoot(): Promise<void> {
    if (typeof window !== 'undefined') {
      const restoredId =
        localStorage.getItem('ancestor_chart_root_person_id') ??
        localStorage.getItem('pedigree_root_person_id') ??
        '';
      const restoredName =
        localStorage.getItem('ancestor_chart_root_person_name') ??
        localStorage.getItem('pedigree_root_person_name') ??
        '';
      if (restoredId) {
        await loadRoot(restoredId, restoredName, generations, false);
        return;
      }
    }

    const firstPage = await api.get<PersonListRow[]>('/api/v1/persons?limit=1&offset=0');
    if (firstPage.length === 0) {
      return;
    }

    await loadRoot(firstPage[0].id, firstPage[0].display_name, generations, false);
  }

  function persistRoot(id: string, name: string): void {
    if (typeof window === 'undefined') {
      return;
    }
    localStorage.setItem('ancestor_chart_root_person_id', id);
    localStorage.setItem('ancestor_chart_root_person_name', name);
    localStorage.setItem('pedigree_root_person_id', id);
    localStorage.setItem('pedigree_root_person_name', name);
  }

  async function loadRoot(
    personId: string,
    displayName: string,
    depth: number,
    appendBreadcrumb: boolean
  ): Promise<void> {
    loading = true;
    error = '';
    tooltipOpen = false;

    try {
      const payload = await ancestorDataStore.fetchAncestors(personId, depth);
      treeData = payload;
      arcs = buildArcs(payload, depth);

      rootPersonId = personId;
      rootPersonName = displayName || payload.display_name;
      persistRoot(rootPersonId, rootPersonName);

      if (appendBreadcrumb) {
        const next = [...breadcrumbs, { personId: rootPersonId, displayName: rootPersonName }];
        breadcrumbs = next;
      } else {
        breadcrumbs = [{ personId: rootPersonId, displayName: rootPersonName }];
      }
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load fan chart';
    } finally {
      loading = false;
    }
  }

  function onGenerationsInput(event: Event): void {
    const input = event.currentTarget as HTMLInputElement;
    generations = Number(input.value);
    if (rootPersonId) {
      void loadRoot(rootPersonId, rootPersonName, generations, false);
    }
  }

  function chooseRoot(result: SearchResult): void {
    searchInput = result.display_name;
    searchResults = [];
    void loadRoot(result.entity_id, result.display_name, generations, false);
  }

  function openPersonDetailFromFan(personId: string): void {
    if (typeof window !== 'undefined') {
      localStorage.setItem(
        'person_nav_context',
        JSON.stringify({ from: 'Fan', href: '/charts/fan', personId: rootPersonId })
      );
    }
    void goto(`/persons/${personId}`);
  }

  function onArcClick(node: ArcDatum): void {
    if (node.personId) {
      openPersonDetailFromFan(node.personId);
    }
  }

  function onRootCircleClick(): void {
    if (!rootPersonId) {
      return;
    }
    openPersonDetailFromFan(rootPersonId);
  }

  function onRootCircleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onRootCircleClick();
    }
  }

  function onArcKeydown(event: KeyboardEvent, node: ArcDatum): void {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onArcClick(node);
    }
  }

  function onArcHover(event: MouseEvent, node: ArcDatum): void {
    if (hideTooltipTimer) {
      clearTimeout(hideTooltipTimer);
      hideTooltipTimer = null;
    }

    tooltipNode = node;
    tooltipOpen = true;

    const rect = chartContainer?.getBoundingClientRect();
    if (rect) {
      tooltipX = event.clientX - rect.left;
      tooltipY = event.clientY - rect.top;
    }
  }

  function onArcLeave(): void {
    hideTooltipTimer = setTimeout(() => {
      tooltipOpen = false;
      tooltipNode = null;
    }, 120);
  }

  function keepTooltipOpen(): void {
    if (hideTooltipTimer) {
      clearTimeout(hideTooltipTimer);
      hideTooltipTimer = null;
    }
  }

  function closeTooltipSoon(): void {
    onArcLeave();
  }

  async function setTooltipNodeAsRoot(): Promise<void> {
    if (!tooltipNode?.personId) {
      return;
    }
    await loadRoot(tooltipNode.personId, tooltipNode.displayName, generations, true);
    tooltipOpen = false;
  }

  function zoomIn(): void {
    zoomScale = Math.min(1.7, Number((zoomScale + 0.1).toFixed(2)));
  }

  function zoomOut(): void {
    zoomScale = Math.max(0.6, Number((zoomScale - 0.1).toFixed(2)));
  }

  function resetZoom(): void {
    zoomScale = 1;
  }

  function openRootInGraph(): void {
    if (!rootPersonId) {
      return;
    }

    if (typeof window !== 'undefined') {
      localStorage.setItem('graph_center_person_id', rootPersonId);
      localStorage.setItem('graph_center_person_name', rootPersonName);
      localStorage.setItem(
        'person_nav_context',
        JSON.stringify({ from: 'Fan', href: '/charts/fan', personId: rootPersonId })
      );
    }

    void goto('/charts/graph');
  }

  $: chartWidth = Math.max(960, Math.floor(chartContainer?.clientWidth ?? 980));
  $: maxRadius = rootRadius + ringWidth * generations + 8;
  $: chartHeight = Math.max(760, Math.ceil(maxRadius + 24 + 24));
  $: centerX = chartWidth / 2;
  $: centerY = Math.ceil(maxRadius + 24);

  onMount(async () => {
    await ensureDefaultRoot();
  });

  onDestroy(() => {
    if (searchDebounce) {
      clearTimeout(searchDebounce);
    }
    if (hideTooltipTimer) {
      clearTimeout(hideTooltipTimer);
    }
  });
</script>

<main class="panel">
  <header class="header">
    <h1>Fan Chart</h1>
    <p>Radial ancestor fan using the shared ancestor source.</p>
  </header>

  <section class="controls">
    <div class="search-box">
      <label for="fan-root-search">Root person</label>
      <input
        id="fan-root-search"
        type="search"
        bind:value={searchInput}
        on:input={onSearchInput}
        placeholder="Search root person…"
      />

      {#if searchResults.length > 0}
        <ul class="search-results">
          {#each searchResults as result}
            <li>
              <button type="button" on:click={() => chooseRoot(result)}>
                <span>{result.display_name}</span>
                {#if result.snippet}
                  <small>{result.snippet}</small>
                {/if}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    <div class="toolbar">
      <label>
        Generations: <strong>{generations}</strong>
        <input
          type="range"
          min="3"
          max="8"
          step="1"
          bind:value={generations}
          on:input={onGenerationsInput}
        />
      </label>

      <button type="button" on:click={zoomOut}>-</button>
      <button type="button" on:click={zoomIn}>+</button>
      <button type="button" class="ghost" on:click={resetZoom}>Reset zoom</button>
      <button type="button" class="ghost" on:click={openRootInGraph} disabled={!rootPersonId}>Open in relationship graph</button>
    </div>
  </section>

  {#if breadcrumbs.length > 0}
    <p class="breadcrumb">
      Root trail:
      {#each breadcrumbs as crumb, idx}
        <span>
          {idx === 0 ? '' : ' → '}
          <button type="button" class="crumb" on:click={() => loadRoot(crumb.personId, crumb.displayName, generations, false)}>
            {crumb.displayName}
          </button>
        </span>
      {/each}
    </p>
  {/if}

  {#if rootPersonId}
    <p class="status">Root: <strong>{rootPersonName || rootPersonId}</strong></p>
  {/if}

  {#if loading}
    <p class="status">Loading fan chart…</p>
  {/if}

  {#if error}
    <p class="error">{error}</p>
  {/if}

  <div class="chart" bind:this={chartContainer}>
    <svg viewBox={`0 0 ${chartWidth} ${chartHeight}`} aria-label="Fan chart">
      <g transform={`translate(${centerX}, ${centerY}) scale(${zoomScale})`}>
        <circle
          cx="0"
          cy="0"
          r={rootRadius}
          fill="#dbeafe"
          stroke="#1e3a8a"
          stroke-width="2"
          class="clickable"
          role="button"
          tabindex="0"
          aria-label="Open root person"
          on:click={onRootCircleClick}
          on:keydown={onRootCircleKeydown}
        />
        <text x="0" y="-4" text-anchor="middle" class="root-text">{rootPersonName || '?'}</text>
        <text x="0" y="14" text-anchor="middle" class="root-subtext">click centre</text>

        {#each arcs as arc}
          <path
            d={arcPath(arc)}
            fill={fillFor(arc.genderHint, arc.isPlaceholder)}
            fill-opacity={confidenceOpacity(arc.confidence, arc.isPlaceholder)}
            stroke={strokeFor(arc.isPlaceholder)}
            stroke-width="1.2"
            stroke-dasharray={arc.isPlaceholder ? '5 4' : 'none'}
            class={arc.personId ? 'clickable' : ''}
            role="button"
            tabindex={arc.personId ? 0 : -1}
            aria-label={arc.personId ? `Open ${arc.displayName}` : `Placeholder ancestor slot ${arc.generation}-${arc.slot}`}
            on:mouseenter={(event) => onArcHover(event, arc)}
            on:mousemove={(event) => onArcHover(event, arc)}
            on:mouseleave={onArcLeave}
            on:click={() => onArcClick(arc)}
            on:keydown={(event) => onArcKeydown(event, arc)}
          />

          {#if !arc.isPlaceholder && canRenderArcLabel(arc)}
            {@const point = centroid(arc, 0.5)}
            {@const split = surnameAndGiven(arc.displayName)}
            <g transform={`translate(${point.x}, ${point.y}) rotate(${labelRotation(arc)})`}>
              <text text-anchor={labelAnchor(arc)} dominant-baseline="middle" class="arc-label surname">{split.surname}</text>
              <text text-anchor={labelAnchor(arc)} dominant-baseline="middle" dy="12" class="arc-label">{truncateGiven(split.given)}</text>
              <text text-anchor={labelAnchor(arc)} dominant-baseline="middle" dy="24" class="arc-label year">
                {arc.birthYear === null ? '?' : arc.birthYear}
              </text>
            </g>
          {/if}
        {/each}
      </g>
    </svg>

    {#if tooltipOpen && tooltipNode}
      <div
        class="tooltip"
        role="tooltip"
        style={`left:${tooltipX + 10}px;top:${tooltipY + 10}px;`}
        on:mouseenter={keepTooltipOpen}
        on:mouseleave={closeTooltipSoon}
      >
        <p><strong>{tooltipNode.displayName}</strong></p>
        <p>Dates: {fullLifeLabel(tooltipNode)}</p>
        {#if tooltipNode.personId}
          <div class="tooltip-actions">
            <button type="button" on:click={() => tooltipNode?.personId && openPersonDetailFromFan(tooltipNode.personId)}>Open</button>
            <button type="button" class="ghost" on:click={setTooltipNodeAsRoot}>Set as root</button>
          </div>
        {/if}
      </div>
    {/if}
  </div>
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

  .header h1 {
    margin: 0;
  }

  .header p {
    margin: 0.35rem 0 0;
    color: #64748b;
  }

  .controls {
    display: grid;
    gap: 0.65rem;
  }

  .search-box {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    max-width: 34rem;
  }

  .search-box label {
    font-size: 0.9rem;
    color: #334155;
  }

  .search-box input,
  .toolbar input[type='range'] {
    border: 1px solid #cbd5e1;
    border-radius: 0.45rem;
    padding: 0.42rem 0.52rem;
    font: inherit;
    background: #fff;
  }

  .search-results {
    position: absolute;
    left: 0;
    right: 0;
    top: calc(100% + 0.2rem);
    margin: 0;
    padding: 0.3rem;
    list-style: none;
    border: 1px solid #e2e8f0;
    border-radius: 0.45rem;
    background: #fff;
    z-index: 20;
    box-shadow: 0 8px 24px rgb(15 23 42 / 16%);
  }

  .search-results button {
    width: 100%;
    border: 0;
    background: transparent;
    padding: 0.45rem;
    border-radius: 0.35rem;
    text-align: left;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 0.18rem;
  }

  .search-results button:hover {
    background: #f8fafc;
  }

  .search-results small {
    color: #64748b;
    font-size: 0.78rem;
  }

  .toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 0.55rem;
    align-items: center;
  }

  .toolbar label {
    display: inline-flex;
    gap: 0.5rem;
    align-items: center;
    color: #334155;
    font-size: 0.9rem;
  }

  button {
    border: 0;
    border-radius: 0.45rem;
    padding: 0.42rem 0.68rem;
    background: #2563eb;
    color: #fff;
    cursor: pointer;
  }

  button.ghost {
    background: #f8fafc;
    color: #0f172a;
    border: 1px solid #cbd5e1;
  }

  .breadcrumb,
  .status {
    margin: 0;
    color: #334155;
  }

  .crumb {
    border: 0;
    background: transparent;
    color: #1d4ed8;
    text-decoration: underline;
    padding: 0;
    cursor: pointer;
  }

  .error {
    margin: 0;
    color: #b91c1c;
  }

  .chart {
    position: relative;
    border: 1px solid #e2e8f0;
    border-radius: 0.6rem;
    background: #f8fafc;
    overflow: hidden;
    min-height: 760px;
  }

  svg {
    width: 100%;
    height: 760px;
    display: block;
  }

  .clickable {
    cursor: pointer;
  }

  .root-text {
    font-size: 0.65rem;
    fill: #0f172a;
    font-weight: 600;
  }

  .root-subtext {
    font-size: 0.58rem;
    fill: #334155;
  }

  .arc-label {
    font-size: 0.54rem;
    fill: #0f172a;
  }

  .arc-label.surname {
    font-weight: 700;
  }

  .arc-label.year {
    fill: #334155;
  }

  .tooltip {
    position: absolute;
    z-index: 40;
    background: #0f172a;
    color: #fff;
    border-radius: 0.35rem;
    padding: 0.42rem 0.5rem;
    font-size: 0.78rem;
    max-width: 20rem;
    box-shadow: 0 10px 28px rgb(15 23 42 / 28%);
  }

  .tooltip p {
    margin: 0;
  }

  .tooltip p + p {
    margin-top: 0.2rem;
  }

  .tooltip-actions {
    margin-top: 0.4rem;
    display: flex;
    gap: 0.4rem;
  }

  .tooltip-actions button {
    padding: 0.28rem 0.5rem;
    font-size: 0.72rem;
  }
</style>
