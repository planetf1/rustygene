<script lang="ts">
  import { goto } from '$app/navigation';
  import { onDestroy, onMount } from 'svelte';
  import * as d3 from 'd3';
  import { api } from '$lib/api';
  import { ancestorDataStore, cloneAncestor, type AncestorApiNode } from '$lib/charts/ancestorStore';

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

  type PedigreeDatum = {
    personId: string | null;
    displayName: string;
    birthYear: number | null;
    deathYear: number | null;
    confidence: number;
    relation: 'self' | 'father' | 'mother' | 'unknown' | 'linked';
    depth: number;
    isUnknown: boolean;
    children: PedigreeDatum[];
    linkedToPersonId?: string;
    appearsCount?: number;
  };

  type RenderNode = {
    id: string;
    personId: string | null;
    linkedToPersonId?: string;
    displayName: string;
    birthYear: number | null;
    deathYear: number | null;
    confidence: number;
    depth: number;
    x: number;
    y: number;
    kind: 'person' | 'unknown' | 'linked';
    appearsCount: number;
    hasMissingParents: boolean;
  };

  let svgElement: SVGSVGElement | null = null;
  let chartContainer: HTMLDivElement | null = null;

  let loading = false;
  let expanding = false;
  let error = '';

  let rootPersonId = '';
  let rootPersonName = '';
  let generations = 4;
  let loadedGenerations = 4;
  let showConfidenceColors = true;

  let searchInput = '';
  let searchResults: SearchResult[] = [];
  let searchDebounce: ReturnType<typeof setTimeout> | null = null;

  let tooltipOpen = false;
  let tooltipX = 0;
  let tooltipY = 0;
  let tooltipText = '';

  let treeData: AncestorApiNode | null = null;
  let renderedNodes: RenderNode[] = [];

  const expandedRoots = new Set<string>();
  let viewportExpandDebounce: ReturnType<typeof setTimeout> | null = null;

  let zoomBehavior: d3.ZoomBehavior<SVGSVGElement, unknown> | null = null;
  let zoomState = d3.zoomIdentity;
  let pedigreeGroup: d3.Selection<SVGGElement, unknown, null, undefined> | null = null;
  let suppressZoomExpand = false;

  const nodeWidth = 168;
  const nodeHeight = 54;
  const rowGap = 18;
  const colGap = 88;

  function truncateLabel(label: string, max = 22): string {
    if (label.length <= max) {
      return label;
    }
    return `${label.slice(0, max - 1)}…`;
  }

  function lifeLine(node: Pick<RenderNode, 'birthYear' | 'deathYear'>): string {
    if (node.birthYear === null && node.deathYear === null) {
      return 'b. ?';
    }
    if (node.birthYear !== null && node.deathYear === null) {
      return `b. ${node.birthYear}`;
    }
    if (node.birthYear === null && node.deathYear !== null) {
      return `? - ${node.deathYear}`;
    }
    return `${node.birthYear} - ${node.deathYear}`;
  }

  function confidenceColor(confidence: number): string {
    if (!showConfidenceColors) {
      return '#94a3b8';
    }
    if (confidence < 0.45) {
      return '#dc2626';
    }
    if (confidence < 0.75) {
      return '#f59e0b';
    }
    return '#16a34a';
  }

  function fillColor(node: RenderNode): string {
    if (node.kind === 'unknown') {
      return '#f8fafc';
    }
    if (node.kind === 'linked') {
      return '#dbeafe';
    }
    return '#ffffff';
  }

  function nodeStrokeDash(node: RenderNode): string {
    if (node.kind === 'unknown') {
      return '6 4';
    }
    if (node.kind === 'linked') {
      return '4 4';
    }
    return 'none';
  }

  function edgeStrokeDash(target: RenderNode): string {
    if (target.kind === 'unknown' || target.kind === 'linked') {
      return '5 4';
    }
    return 'none';
  }

  async function fetchAncestors(personId: string, depth: number): Promise<AncestorApiNode> {
    return ancestorDataStore.fetchAncestors(personId, depth);
  }

  async function searchPeople(): Promise<void> {
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
      void searchPeople();
    }, 220);
  }

  function onGenerationsChange(event: Event): void {
    const input = event.currentTarget as HTMLSelectElement;
    generations = Number(input.value);
    if (rootPersonId) {
      void loadRoot(rootPersonId, rootPersonName, generations);
    }
  }

  async function ensureDefaultRoot(): Promise<void> {
    if (typeof window !== 'undefined') {
      const restoredId = localStorage.getItem('pedigree_root_person_id') ?? '';
      const restoredName = localStorage.getItem('pedigree_root_person_name') ?? '';
      if (restoredId) {
        rootPersonId = restoredId;
        rootPersonName = restoredName;
        await loadRoot(restoredId, restoredName, generations);
        return;
      }
    }

    const rows = await api.get<PersonListRow[]>('/api/v1/persons?limit=1&offset=0');
    if (rows.length === 0) {
      return;
    }

    rootPersonId = rows[0].id;
    rootPersonName = rows[0].display_name;
    await loadRoot(rows[0].id, rows[0].display_name, generations);
  }

  async function loadRoot(personId: string, displayName: string, depth: number): Promise<void> {
    loading = true;
    error = '';
    tooltipOpen = false;
    expandedRoots.clear();

    try {
      const payload = await fetchAncestors(personId, depth);
      treeData = cloneAncestor(payload);
      loadedGenerations = depth;
      rootPersonId = personId;
      rootPersonName = displayName || payload.display_name;

      if (typeof window !== 'undefined') {
        localStorage.setItem('pedigree_root_person_id', rootPersonId);
        localStorage.setItem('pedigree_root_person_name', rootPersonName);
      }

      renderChart();
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load pedigree chart';
    } finally {
      loading = false;
    }
  }

  function chooseRoot(result: SearchResult): void {
    searchInput = result.display_name;
    searchResults = [];
    void loadRoot(result.entity_id, result.display_name, generations);
  }

  function openPersonDetailFromPedigree(personId: string): void {
    if (typeof window !== 'undefined') {
      localStorage.setItem(
        'person_nav_context',
        JSON.stringify({ from: 'Pedigree', href: '/charts/pedigree', personId: rootPersonId })
      );
    }
    void goto(`/persons/${personId}`);
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
        JSON.stringify({ from: 'Pedigree', href: '/charts/pedigree', personId: rootPersonId })
      );
    }

    void goto('/charts/graph');
  }

  function toPedigreeDatum(
    node: AncestorApiNode,
    depth: number,
    maxDepth: number,
    relation: PedigreeDatum['relation']
  ): PedigreeDatum {
    const children: PedigreeDatum[] = [];
    if (depth < maxDepth) {
      if (node.father) {
        children.push(toPedigreeDatum(node.father, depth + 1, maxDepth, 'father'));
      } else {
        children.push({
          personId: null,
          displayName: '?',
          birthYear: null,
          deathYear: null,
          confidence: 0.25,
          relation: 'unknown',
          depth: depth + 1,
          isUnknown: true,
          children: []
        });
      }

      if (node.mother) {
        children.push(toPedigreeDatum(node.mother, depth + 1, maxDepth, 'mother'));
      } else {
        children.push({
          personId: null,
          displayName: '?',
          birthYear: null,
          deathYear: null,
          confidence: 0.25,
          relation: 'unknown',
          depth: depth + 1,
          isUnknown: true,
          children: []
        });
      }
    }

    return {
      personId: node.person_id,
      displayName: node.display_name,
      birthYear: node.birth_year,
      deathYear: node.death_year,
      confidence: node.confidence,
      relation,
      depth,
      isUnknown: false,
      children
    };
  }

  function calculateViewport(): { width: number; height: number } {
    const width = Math.max(980, Math.floor(chartContainer?.clientWidth ?? 980));
    const containerHeight = Math.floor(chartContainer?.clientHeight ?? 0);
    const height = Math.max(680, containerHeight || 680);
    return { width, height };
  }

  function renderChart(): void {
    if (!svgElement || !treeData) {
      return;
    }

    const { width, height } = calculateViewport();
    const rootDatum = toPedigreeDatum(treeData, 0, loadedGenerations, 'self');
    const hierarchyRoot = d3.hierarchy<PedigreeDatum>(rootDatum, (item) => item.children);

    const treeLayout = d3
      .tree<PedigreeDatum>()
      .nodeSize([nodeHeight + rowGap, nodeWidth + colGap])
      .separation((left, right) => (left.parent === right.parent ? 1 : 1.2));

    treeLayout(hierarchyRoot);

    const personCounts = new Map<string, number>();
    hierarchyRoot.descendants().forEach((node) => {
      const personId = node.data.personId;
      if (!personId) {
        return;
      }
      personCounts.set(personId, (personCounts.get(personId) ?? 0) + 1);
    });

    const seenPersons = new Set<string>();
    const nodeByPersonId = new Map<string, RenderNode>();

    renderedNodes = hierarchyRoot.descendants().map((node, index) => {
      const data = node.data;
      let kind: RenderNode['kind'] = 'person';
      let linkedToPersonId: string | undefined;

      if (data.isUnknown) {
        kind = 'unknown';
      } else if (data.personId) {
        if (seenPersons.has(data.personId)) {
          kind = 'linked';
          linkedToPersonId = data.personId;
        } else {
          seenPersons.add(data.personId);
        }
      }

      const count = data.personId ? personCounts.get(data.personId) ?? 1 : 1;
      const y = node.depth * (nodeWidth + colGap) + 70;
      const x = (node.x ?? 0) + height / 2;

      const renderNode: RenderNode = {
        id: `${data.personId ?? 'unknown'}-${index}`,
        personId: data.personId,
        linkedToPersonId,
        displayName: data.isUnknown
          ? '?'
          : kind === 'linked'
            ? '↪ linked ancestor'
            : data.displayName,
        birthYear: data.birthYear,
        deathYear: data.deathYear,
        confidence: data.confidence,
        depth: node.depth,
        x,
        y,
        kind,
        appearsCount: count,
        hasMissingParents:
          Boolean(data.personId) &&
          kind === 'person' &&
          data.children.some((child) => child.isUnknown)
      };

      if (renderNode.personId && renderNode.kind === 'person') {
        nodeByPersonId.set(renderNode.personId, renderNode);
      }

      return renderNode;
    });

    const selection = d3.select(svgElement);
    selection.selectAll('*').remove();
    selection.attr('viewBox', `0 0 ${width} ${height}`);

    if (!zoomBehavior) {
      zoomBehavior = d3
        .zoom<SVGSVGElement, unknown>()
        .scaleExtent([0.4, 2.5])
        .on('zoom', (event) => {
          zoomState = event.transform;
          pedigreeGroup?.attr('transform', zoomState.toString());

          if (suppressZoomExpand) {
            return;
          }

          if (viewportExpandDebounce) {
            clearTimeout(viewportExpandDebounce);
          }
          viewportExpandDebounce = setTimeout(() => {
            void expandNearViewportEdge();
          }, 260);
        });
      selection.call(zoomBehavior);
    }

    pedigreeGroup = selection.append('g').attr('class', 'pedigree-root');
    pedigreeGroup.attr('transform', zoomState.toString());

    const links = hierarchyRoot.links();
    pedigreeGroup
      .append('g')
      .attr('class', 'edges')
      .selectAll('path')
      .data(links)
      .join('path')
      .attr('fill', 'none')
      .attr('stroke', '#334155')
      .attr('stroke-width', 1.6)
      .attr('stroke-dasharray', (link) => {
        const targetNode = renderedNodes.find((node) => node.x === (link.target.x ?? 0) + height / 2 && node.y === link.target.depth * (nodeWidth + colGap) + 70);
        return targetNode ? edgeStrokeDash(targetNode) : 'none';
      })
      .attr('d', (link) => {
        const sx = (link.source.x ?? 0) + height / 2;
        const sy = link.source.depth * (nodeWidth + colGap) + 70 + nodeWidth / 2;
        const tx = (link.target.x ?? 0) + height / 2;
        const ty = link.target.depth * (nodeWidth + colGap) + 70 - nodeWidth / 2;
        return `M ${sy} ${sx} H ${sy + 24} V ${tx} H ${ty}`;
      });

    const nodeGroups = pedigreeGroup
      .append('g')
      .attr('class', 'nodes')
      .selectAll('g')
      .data(renderedNodes)
      .join('g')
      .attr('transform', (node) => `translate(${node.y - nodeWidth / 2}, ${node.x - nodeHeight / 2})`)
      .style('cursor', 'pointer')
      .on('mouseenter', (event, node) => {
        const fullLabel = node.kind === 'linked' && node.linkedToPersonId
          ? `Linked to ${node.linkedToPersonId}`
          : `${node.displayName} · ${lifeLine(node)} · place: n/a`;
        tooltipOpen = true;
        tooltipX = event.clientX;
        tooltipY = event.clientY;
        tooltipText = fullLabel;
      })
      .on('mousemove', (event) => {
        tooltipX = event.clientX;
        tooltipY = event.clientY;
      })
      .on('mouseleave', () => {
        tooltipOpen = false;
      })
      .on('click', (_event, node) => {
        if (node.kind === 'linked' && node.linkedToPersonId) {
          const target = nodeByPersonId.get(node.linkedToPersonId);
          if (target) {
            centerOnNode(target);
            return;
          }

          void loadRoot(node.linkedToPersonId, node.displayName, generations);
          return;
        }

        if (node.personId) {
          openPersonDetailFromPedigree(node.personId);
        }
      });

    nodeGroups
      .append('rect')
      .attr('rx', 8)
      .attr('ry', 8)
      .attr('width', nodeWidth)
      .attr('height', nodeHeight)
      .attr('fill', (node) => fillColor(node))
      .attr('stroke', (node) => confidenceColor(node.confidence))
      .attr('stroke-width', 2.2)
      .attr('stroke-dasharray', (node) => nodeStrokeDash(node));

    nodeGroups
      .append('text')
      .attr('x', 10)
      .attr('y', 20)
      .attr('font-size', 14)
      .attr('font-weight', 600)
      .attr('fill', '#0f172a')
      .text((node) => truncateLabel(node.displayName));

    nodeGroups
      .append('text')
      .attr('x', 10)
      .attr('y', 38)
      .attr('font-size', 12)
      .attr('fill', '#475569')
      .text((node) => lifeLine(node));

    nodeGroups
      .filter((node) => node.kind === 'person' && node.appearsCount > 1)
      .append('text')
      .attr('x', nodeWidth - 8)
      .attr('y', 12)
      .attr('text-anchor', 'end')
      .attr('font-size', 9)
      .attr('fill', '#1d4ed8')
      .text((node) => `appears ${node.appearsCount}x`);

    if (zoomBehavior) {
      suppressZoomExpand = true;
      selection.call(zoomBehavior.transform, zoomState);
      requestAnimationFrame(() => {
        suppressZoomExpand = false;
      });
    }
  }

  function centerOnNode(node: RenderNode): void {
    if (!svgElement || !zoomBehavior) {
      return;
    }

    const { width, height } = calculateViewport();
    const targetX = width * 0.24;
    const targetY = height * 0.5;

    const tx = targetX - node.y * zoomState.k;
    const ty = targetY - node.x * zoomState.k;

    const transform = d3.zoomIdentity.translate(tx, ty).scale(zoomState.k);
    d3.select(svgElement).transition().duration(260).call(zoomBehavior.transform, transform);
  }

  function mergeAncestorSubtree(root: AncestorApiNode, subtree: AncestorApiNode): void {
    if (root.person_id === subtree.person_id) {
      if (!root.father && subtree.father) {
        root.father = cloneAncestor(subtree.father);
      }
      if (!root.mother && subtree.mother) {
        root.mother = cloneAncestor(subtree.mother);
      }
    }

    if (root.father) {
      mergeAncestorSubtree(root.father, subtree);
    }
    if (root.mother) {
      mergeAncestorSubtree(root.mother, subtree);
    }
  }

  async function expandNearViewportEdge(): Promise<void> {
    if (!svgElement || !treeData || expanding) {
      return;
    }

    const { width } = calculateViewport();
    const viewportRight = (width - zoomState.x) / zoomState.k;
    const threshold = viewportRight - 140;

    const candidate = renderedNodes
      .filter((node) => node.kind === 'person')
      .filter((node) => node.hasMissingParents)
      .filter((node) => node.y >= threshold)
      .filter((node) => node.personId && !expandedRoots.has(node.personId))
      .sort((left, right) => right.depth - left.depth)[0];

    if (!candidate?.personId) {
      return;
    }

    expanding = true;
    expandedRoots.add(candidate.personId);

    try {
      const subtree = await fetchAncestors(candidate.personId, 2);
      const next = cloneAncestor(treeData);
      mergeAncestorSubtree(next, subtree);
      treeData = next;
      loadedGenerations = Math.min(8, loadedGenerations + 1);
      renderChart();
    } catch {
      // keep UI responsive; root-level error is intentionally suppressed for background expansion
    } finally {
      expanding = false;
    }
  }

  function zoomIn(): void {
    if (!svgElement || !zoomBehavior) {
      return;
    }
    d3.select(svgElement).transition().duration(160).call(zoomBehavior.scaleBy, 1.2);
  }

  function zoomOut(): void {
    if (!svgElement || !zoomBehavior) {
      return;
    }
    d3.select(svgElement).transition().duration(160).call(zoomBehavior.scaleBy, 1 / 1.2);
  }

  function resetZoom(): void {
    if (!svgElement || !zoomBehavior) {
      return;
    }
    const transform = d3.zoomIdentity.translate(42, 0).scale(1);
    d3.select(svgElement).transition().duration(220).call(zoomBehavior.transform, transform);
  }

  function rerenderWithStyle(): void {
    if (treeData) {
      renderChart();
    }
  }

  onMount(async () => {
    await ensureDefaultRoot();
    resetZoom();
  });

  onDestroy(() => {
    if (searchDebounce) {
      clearTimeout(searchDebounce);
    }
    if (viewportExpandDebounce) {
      clearTimeout(viewportExpandDebounce);
    }
  });
</script>

<main class="panel">
  <header class="header">
    <h1>Pedigree Chart</h1>
    <p>D3 pedigree view with zoom/pan and async ancestor extension.</p>
  </header>

  <section class="guide" aria-label="How to read this chart">
    <h2>How to follow the tree</h2>
    <p>
      This view follows <strong>parent → child lineage</strong>. In cases of divorce or separation,
      ancestry paths still follow biological/adoptive parent links, not partner status.
    </p>
    <ul>
      <li><strong>Solid card</strong>: known ancestor node.</li>
      <li><strong>Dashed card with “?”</strong>: parent unknown at current depth.</li>
      <li><strong>↪ linked ancestor</strong>: same person appears elsewhere in the tree; click to jump.</li>
    </ul>
  </section>

  <section class="controls">
    <div class="search-box">
      <label for="root-search">Center on person</label>
      <input
        id="root-search"
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
        Generations
        <select bind:value={generations} on:change={onGenerationsChange}>
          {#each [1, 2, 3, 4, 5, 6, 7, 8] as depth}
            <option value={depth}>{depth}</option>
          {/each}
        </select>
      </label>

      <label class="inline-check">
        <input type="checkbox" bind:checked={showConfidenceColors} on:change={rerenderWithStyle} />
        <span>Show confidence colours</span>
      </label>

      <button type="button" on:click={zoomIn}>Zoom in</button>
      <button type="button" on:click={zoomOut}>Zoom out</button>
      <button type="button" class="ghost" on:click={resetZoom}>Reset</button>
      <button type="button" class="ghost" on:click={openRootInGraph} disabled={!rootPersonId}>Open in relationship graph</button>
    </div>
  </section>

  {#if rootPersonId}
    <p class="status">Root: <strong>{rootPersonName || rootPersonId}</strong> · loaded depth: {loadedGenerations}</p>
  {/if}

  {#if loading}
    <p class="status">Loading pedigree…</p>
  {/if}

  {#if error}
    <p class="error">{error}</p>
  {/if}

  <div class="chart" bind:this={chartContainer}>
    <svg bind:this={svgElement} aria-label="Pedigree chart"></svg>
    {#if tooltipOpen}
      <div class="tooltip" style={`left:${tooltipX + 10}px;top:${tooltipY + 10}px;`}>
        {tooltipText}
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

  .guide {
    border: 1px solid #dbe3f1;
    background: #f8fbff;
    border-radius: 0.65rem;
    padding: 0.75rem 0.85rem;
  }

  .guide h2 {
    margin: 0 0 0.35rem;
    font-size: 0.98rem;
  }

  .guide p {
    margin: 0;
    color: #334155;
    font-size: 0.9rem;
  }

  .guide ul {
    margin: 0.5rem 0 0;
    padding-left: 1rem;
    color: #334155;
    font-size: 0.86rem;
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
  select {
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
    gap: 0.4rem;
    align-items: center;
    color: #334155;
    font-size: 0.9rem;
  }

  .inline-check {
    margin-left: 0.25rem;
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

  .status {
    margin: 0;
    color: #334155;
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
    min-height: 680px;
  }

  svg {
    width: 100%;
    height: 680px;
    display: block;
  }

  .tooltip {
    position: fixed;
    z-index: 40;
    pointer-events: none;
    background: #0f172a;
    color: #fff;
    border-radius: 0.35rem;
    padding: 0.36rem 0.46rem;
    font-size: 0.78rem;
    max-width: 26rem;
  }
</style>
