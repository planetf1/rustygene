<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { api } from '$lib/api';

  type GraphNode = {
    id: string;
    label: string;
    type: 'person' | 'family' | 'event' | 'unknown';
    birth_year: number | null;
    death_year: number | null;
  };

  type GraphEdge = {
    source: string;
    target: string;
    label: string;
    edge_type: string;
  };

  type GraphResponse = {
    nodes: GraphNode[];
    edges: GraphEdge[];
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

  type PersonListRow = {
    id: string;
    display_name: string;
  };

  type ColorMode = 'confidence' | 'gender';
  type LayoutMode = 'cose-bilkent' | 'cola' | 'breadthfirst' | 'circle';

  type ContextMenuState = {
    open: boolean;
    nodeId: string;
    x: number;
    y: number;
  };

  type TooltipState = {
    open: boolean;
    x: number;
    y: number;
    text: string;
  };

  type CytoscapeLike = {
    use: (extension: unknown) => void;
    (options: Record<string, unknown>): CytoscapeCoreLike;
  };

  type CytoscapeCoreLike = {
    destroy: () => void;
    on: (event: string, selectorOrHandler: string | ((event: unknown) => void), handler?: (event: unknown) => void) => void;
    off: (event: string) => void;
    add: (elements: { nodes: Array<{ data: Record<string, unknown> }>; edges: Array<{ data: Record<string, unknown> }> }) => void;
    getElementById: (id: string) => CytoscapeElementLike;
    nodes: () => CytoscapeCollectionLike;
    edges: () => CytoscapeCollectionLike;
    elements: () => CytoscapeCollectionLike;
    fit: (eles?: CytoscapeCollectionLike, padding?: number) => void;
    layout: (options: Record<string, unknown>) => { run: () => void };
    extent: () => { x1: number; y1: number; x2: number; y2: number; w: number; h: number };
    container: () => HTMLElement | null;
    style: () => {
      selector: (query: string) => {
        style: (declarations: Record<string, string>) => {
          update: () => void;
        };
      };
    };
  };

  type CytoscapeCollectionLike = {
    forEach: (callback: (item: CytoscapeElementLike) => void) => void;
    map: <T>(callback: (item: CytoscapeElementLike) => T) => T[];
    length: number;
    addClass: (className: string) => void;
    removeClass: (className: string) => void;
    lock: () => void;
    unlock: () => void;
    filter: (predicate: (item: CytoscapeElementLike) => boolean) => CytoscapeCollectionLike;
  };

  type CytoscapeElementLike = {
    id: () => string;
    data: (name?: string, value?: unknown) => unknown;
    isNode: () => boolean;
    isEdge: () => boolean;
    position: () => { x: number; y: number };
    addClass: (className: string) => void;
    removeClass: (className: string) => void;
    connectedEdges: () => CytoscapeCollectionLike;
    source: () => CytoscapeElementLike;
    target: () => CytoscapeElementLike;
    json: () => { data: Record<string, unknown> };
  };

  let cytoscapeCtor: CytoscapeLike | null = null;
  let graphContainer: HTMLDivElement | null = null;
  let cy: CytoscapeCoreLike | null = null;
  let loading = false;
  let error = '';
  let searchInput = '';
  let searchResults: SearchResult[] = [];
  let searchDebounce: ReturnType<typeof setTimeout> | null = null;
  let selectedCenterId = '';
  let selectedCenterName = '';
  let selectedNodeId = '';
  let sidePanelOpen = false;
  let selectedNodeSummary: GraphNode | null = null;
  let colorMode: ColorMode = 'confidence';
  let hideEventEdges = true;
  let layoutMode: LayoutMode = 'cose-bilkent';
  let contextMenu: ContextMenuState = { open: false, nodeId: '', x: 0, y: 0 };
  let edgeTooltip: TooltipState = { open: false, x: 0, y: 0, text: '' };

  const edgeIds = new Set<string>();
  const expandedNodeIds = new Set<string>();
  const loadingNodeIds = new Set<string>();
  const hiddenNodeIds = new Set<string>();
  const nodeSeenFrom = new Map<string, Set<string>>();
  const nodesById = new Map<string, GraphNode>();
  const nodesWithManualPlacement = new Set<string>();

  let viewportDebounce: ReturnType<typeof setTimeout> | null = null;
  let clickDebounce: ReturnType<typeof setTimeout> | null = null;
  let layoutDebounce: ReturnType<typeof setTimeout> | null = null;
  let isApplyingLayout = false;
  let lastTapNodeId = '';
  let lastTapAt = 0;

  function lifeLabel(node: GraphNode): string {
    const from = node.birth_year === null ? '?' : String(node.birth_year);
    const to = node.death_year === null ? '?' : String(node.death_year);
    return `${from}–${to}`;
  }

  function mergedSuffix(nodeId: string): string {
    const sources = nodeSeenFrom.get(nodeId);
    return sources && sources.size > 1 ? ' ⟲ merged' : '';
  }

  function displayLabel(node: GraphNode): string {
    return `${node.label}\n${lifeLabel(node)}${mergedSuffix(node.id)}`;
  }

  function nodeConfidence(node: GraphNode): number {
    if (node.birth_year !== null && node.death_year !== null) {
      return 1;
    }
    if (node.birth_year !== null || node.death_year !== null) {
      return 0.7;
    }
    return 0.5;
  }

  function edgeIdFor(edge: GraphEdge): string {
    return `${edge.source}|${edge.target}|${edge.edge_type}|${edge.label}`;
  }

  async function loadCytoscape(): Promise<void> {
    if (cytoscapeCtor) {
      return;
    }

    const [{ default: cytoscape }, { default: cola }, { default: coseBilkent }] = await Promise.all([
      import('cytoscape'),
      import('cytoscape-cola'),
      import('cytoscape-cose-bilkent')
    ]);

    cytoscape.use(cola);
    cytoscape.use(coseBilkent);
    cytoscapeCtor = cytoscape as unknown as CytoscapeLike;
  }

  function buildLayoutConfig(mode: LayoutMode): Record<string, unknown> {
    switch (mode) {
      case 'cose-bilkent':
        return {
          name: 'cose-bilkent',
          animate: false,
          fit: false,
          randomize: false,
          padding: 8,
          nodeSpacing: 10,
          edgeElasticity: 0.5
        };
      case 'cola':
        return {
          name: 'cola',
          animate: false,
          fit: false,
          randomize: false,
          padding: 8,
          infinite: false,
          nodeSpacing: 8
        };
      case 'breadthfirst':
        return {
          name: 'breadthfirst',
          animate: false,
          fit: false,
          directed: true,
          spacingFactor: 0.75,
          padding: 8
        };
      case 'circle':
        return {
          name: 'circle',
          animate: false,
          fit: false,
          spacingFactor: 0.7,
          padding: 8
        };
    }
  }

  function refreshNodeLabels(): void {
    if (!cy) {
      return;
    }

    cy.nodes().forEach((node) => {
      const nodeId = node.id();
      const summary = nodesById.get(nodeId);
      if (!summary) {
        return;
      }

      node.data('label', displayLabel(summary));
      node.data('confidence', nodeConfidence(summary));
      node.data('color_mode', colorMode);
      node.data('merged', (nodeSeenFrom.get(nodeId)?.size ?? 0) > 1 ? 1 : 0);
    });
  }

  function refreshEdgeVisibility(): void {
    if (!cy) {
      return;
    }

    cy.edges().forEach((edge) => {
      const edgeType = String(edge.data('edge_type') ?? '');
      const sourceId = String(edge.data('source') ?? '');
      const targetId = String(edge.data('target') ?? '');
      const hiddenByEvent = hideEventEdges && edgeType === 'event_participant';
      const hiddenByCollapse = hiddenNodeIds.has(sourceId) || hiddenNodeIds.has(targetId);
      if (hiddenByEvent || hiddenByCollapse) {
        edge.addClass('hidden');
      } else {
        edge.removeClass('hidden');
      }
    });

    cy.nodes().forEach((node) => {
      if (hiddenNodeIds.has(node.id())) {
        node.addClass('hidden');
      } else {
        node.removeClass('hidden');
      }
    });
  }

  function applyLayout(newNodeIds: Set<string>): void {
    if (!cy || isApplyingLayout) {
      return;
    }

    if (layoutDebounce) {
      clearTimeout(layoutDebounce);
    }

    // Debounce layout to prevent excessive re-renders
    layoutDebounce = setTimeout(() => {
      if (!cy) {
        return;
      }

      isApplyingLayout = true;
      try {
        const layout = buildLayoutConfig(layoutMode);
        const existing = cy.nodes().filter((node) => !newNodeIds.has(node.id()));
        const newcomers = cy.nodes().filter((node) => newNodeIds.has(node.id()));

        existing.lock();
        newcomers.unlock();

        cy.layout(layout).run();

        existing.unlock();
        nodesWithManualPlacement.clear();
        cy.nodes().forEach((node) => {
          nodesWithManualPlacement.add(node.id());
        });
      } finally {
        isApplyingLayout = false;
      }
    }, 280);
  }

  function styleForNode(): string {
    if (colorMode === 'gender') {
      return '#93c5fd';
    }
    return 'mapData(confidence, 0, 1, #ef4444, #16a34a)';
  }

  async function initializeGraphCanvas(): Promise<void> {
    if (!graphContainer) {
      return;
    }

    await loadCytoscape();
    if (!cytoscapeCtor) {
      return;
    }

    cy = cytoscapeCtor({
      container: graphContainer,
      elements: [],
      style: [
        {
          selector: 'node',
          style: {
            label: 'data(label)',
            'background-color': styleForNode(),
            shape: 'ellipse',
            width: 105,
            height: 105,
            color: '#0f172a',
            'font-size': 17,
            'font-weight': 600,
            'text-wrap': 'wrap',
            'text-max-width': 160,
            'text-valign': 'center',
            'text-halign': 'center',
            'text-margin-y': 0,
            'border-color': '#0f172a',
            'border-width': 'mapData(merged, 0, 1, 1, 4)',
            'line-height': 1.2
          }
        },
        {
          selector: 'node[type = "family"]',
          style: {
            shape: 'diamond',
            'background-color': '#fde68a'
          }
        },
        {
          selector: 'node[type = "unknown"]',
          style: {
            shape: 'diamond',
            label: '?',
            'background-color': '#e2e8f0',
            'border-style': 'dashed'
          }
        },
        {
          selector: 'node.loading',
          style: {
            'border-color': '#f59e0b',
            'border-width': 5
          }
        },
        {
          selector: 'node.hidden',
          style: {
            display: 'none'
          }
        },
        {
          selector: 'edge',
          style: {
            width: 1.2,
            label: 'data(label)',
            'font-size': 12,
            color: '#334155',
            'text-background-color': '#ffffff',
            'text-background-opacity': 1,
            'text-background-padding': 2,
            'curve-style': 'straight',
            'target-arrow-shape': 'none',
            'line-color': '#94a3b8',
            'target-arrow-color': '#334155',
            'control-point-step-size': 40
          }
        },
        {
          selector: 'edge[edge_type = "parent_of"]',
          style: {
            'line-color': '#0f172a',
            'target-arrow-shape': 'triangle',
            'target-arrow-color': '#0f172a',
            'line-style': 'solid'
          }
        },
        {
          selector: 'edge[edge_type = "child_of"]',
          style: {
            'line-color': '#334155',
            'target-arrow-shape': 'triangle',
            'target-arrow-color': '#334155',
            'line-style': 'solid'
          }
        },
        {
          selector: 'edge[edge_type = "partner"]',
          style: {
            'line-color': '#64748b',
            'source-arrow-shape': 'triangle',
            'target-arrow-shape': 'triangle',
            'source-arrow-color': '#64748b',
            'target-arrow-color': '#64748b'
          }
        },
        {
          selector: 'edge[edge_type = "event_participant"]',
          style: {
            'line-style': 'dashed',
            'line-color': '#64748b',
            width: 1.5
          }
        },
        {
          selector: 'edge.hidden',
          style: {
            display: 'none'
          }
        },
        {
          selector: ':selected',
          style: {
            'border-color': '#2563eb',
            'border-width': 4
          }
        }
      ],
      layout: { name: 'cose-bilkent', animate: false, fit: true }
    });

    wireGraphInteractions();
  }

  function wireGraphInteractions(): void {
    if (!cy) {
      return;
    }

    cy.on('tap', (event) => {
      const target = (event as { target?: CytoscapeElementLike }).target;
      if (!target || !target.isNode || !target.isNode()) {
        contextMenu = { open: false, nodeId: '', x: 0, y: 0 };
      }
    });

    cy.on('tap', 'node', (event) => {
      const node = (event as { target: CytoscapeElementLike }).target;
      const nodeId = node.id();
      const now = Date.now();
      const isDoubleTap = lastTapNodeId === nodeId && now - lastTapAt < 280;

      if (clickDebounce) {
        clearTimeout(clickDebounce);
        clickDebounce = null;
      }

      if (isDoubleTap) {
        lastTapNodeId = '';
        lastTapAt = 0;
        void expandFromNode(nodeId, true);
        return;
      }

      lastTapNodeId = nodeId;
      lastTapAt = now;
      clickDebounce = setTimeout(() => {
        openSummaryPanel(nodeId);
      }, 220);
    });

    cy.on('cxttap', 'node', (event) => {
      const node = (event as { target: CytoscapeElementLike; originalEvent?: MouseEvent }).target;
      const original = (event as { originalEvent?: MouseEvent }).originalEvent;
      const container = cy?.container();
      if (!container || !original) {
        return;
      }

      const rect = container.getBoundingClientRect();
      contextMenu = {
        open: true,
        nodeId: node.id(),
        x: original.clientX - rect.left,
        y: original.clientY - rect.top
      };
    });

    cy.on('mouseover', 'edge', (event) => {
      const edge = (event as { target: CytoscapeElementLike; originalEvent?: MouseEvent }).target;
      const original = (event as { originalEvent?: MouseEvent }).originalEvent;
      if (!original || !cy) {
        return;
      }

      const label = String(edge.data('label') ?? 'relationship');
      const type = String(edge.data('edge_type') ?? 'unknown');
      const container = cy.container();
      if (!container) {
        return;
      }

      const rect = container.getBoundingClientRect();
      edgeTooltip = {
        open: true,
        x: original.clientX - rect.left,
        y: original.clientY - rect.top,
        text: `${label} (${type}) · confidence: n/a`
      };
    });

    cy.on('mouseout', 'edge', () => {
      edgeTooltip = { open: false, x: 0, y: 0, text: '' };
    });

    cy.on('pan zoom dragfree', () => {
      if (viewportDebounce) {
        clearTimeout(viewportDebounce);
      }
      viewportDebounce = setTimeout(() => {
        void expandViewportBoundary();
      }, 1000);
    });
  }

  function openSummaryPanel(nodeId: string): void {
    selectedNodeId = nodeId;
    selectedNodeSummary = nodesById.get(nodeId) ?? null;
    sidePanelOpen = selectedNodeSummary !== null;
    contextMenu = { open: false, nodeId: '', x: 0, y: 0 };
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

  async function ensureInitialCenter(): Promise<void> {
    if (typeof window !== 'undefined') {
      const restored = localStorage.getItem('graph_center_person_id') ?? '';
      const restoredName = localStorage.getItem('graph_center_person_name') ?? '';
      if (restored) {
        selectedCenterId = restored;
        selectedCenterName = restoredName;
        await loadGraph(restored, 3, restored);
        return;
      }
    }

    const firstPage = await api.get<PersonListRow[]>('/api/v1/persons?limit=1&offset=0');
    if (firstPage.length === 0) {
      return;
    }

    selectedCenterId = firstPage[0].id;
    selectedCenterName = firstPage[0].display_name;
    await loadGraph(firstPage[0].id, 3, firstPage[0].id);
  }

  function clearGraphState(): void {
    edgeIds.clear();
    expandedNodeIds.clear();
    loadingNodeIds.clear();
    hiddenNodeIds.clear();
    nodeSeenFrom.clear();
    nodesById.clear();
    nodesWithManualPlacement.clear();
    selectedNodeSummary = null;
    sidePanelOpen = false;
    selectedNodeId = '';

    if (cy) {
      cy.elements().removeClass('hidden');
      const all = cy.nodes();
      all.forEach((node) => {
        node.connectedEdges().removeClass('hidden');
      });
    }
  }

  async function loadGraph(centerId: string, radius: number, sourceRoot: string): Promise<void> {
    if (!cy) {
      return;
    }

    loading = true;
    error = '';

    try {
      const payload = await api.get<GraphResponse>(
        `/api/v1/graph/network/${centerId}?radius=${radius}`
      );

      const newNodeIds = mergePayload(payload, sourceRoot);
      if (newNodeIds.size > 0) {
        applyLayout(newNodeIds);
      }

      refreshNodeLabels();
      refreshEdgeVisibility();
      cy.fit(undefined, 55);

      expandedNodeIds.add(centerId);
      if (typeof window !== 'undefined') {
        localStorage.setItem('graph_center_person_id', selectedCenterId);
        localStorage.setItem('graph_center_person_name', selectedCenterName);
      }
    } catch (err) {
      error = err instanceof Error ? err.message : 'Unable to load relationship graph';
    } finally {
      loading = false;
    }
  }

  function mergePayload(payload: GraphResponse, sourceRoot: string): Set<string> {
    if (!cy) {
      return new Set<string>();
    }

    const nodesToAdd: Array<{ data: Record<string, unknown> }> = [];
    const edgesToAdd: Array<{ data: Record<string, unknown> }> = [];
    const newNodeIds = new Set<string>();

    for (const node of payload.nodes) {
      nodesById.set(node.id, node);
      const seen = nodeSeenFrom.get(node.id) ?? new Set<string>();
      seen.add(sourceRoot);
      nodeSeenFrom.set(node.id, seen);

      const exists = cy.getElementById(node.id);
      if (!exists || !exists.id || exists.id() === '') {
        nodesToAdd.push({
          data: {
            id: node.id,
            label: displayLabel(node),
            type: node.type ?? 'person',
            birth_year: node.birth_year,
            death_year: node.death_year,
            confidence: nodeConfidence(node),
            merged: seen.size > 1 ? 1 : 0,
            color_mode: colorMode
          }
        });
        newNodeIds.add(node.id);
      }
    }

    for (const edge of payload.edges) {
      const edgeId = edgeIdFor(edge);
      if (edgeIds.has(edgeId)) {
        continue;
      }

      edgeIds.add(edgeId);
      edgesToAdd.push({
        data: {
          id: edgeId,
          source: edge.source,
          target: edge.target,
          label: edge.label,
          edge_type: edge.edge_type
        }
      });
    }

    if (nodesToAdd.length > 0 || edgesToAdd.length > 0) {
      cy.add({
        nodes: nodesToAdd,
        edges: edgesToAdd
      });
    }

    return newNodeIds;
  }

  async function chooseCenter(result: SearchResult): Promise<void> {
    searchInput = result.display_name;
    searchResults = [];
    selectedCenterId = result.entity_id;
    selectedCenterName = result.display_name;

    clearGraphState();
    cy?.destroy();
    cy = null;

    await initializeGraphCanvas();
    await loadGraph(result.entity_id, 3, result.entity_id);
  }

  async function expandFromNode(nodeId: string, focusAfter: boolean): Promise<void> {
    if (!cy) {
      return;
    }
    if (expandedNodeIds.has(nodeId) || loadingNodeIds.has(nodeId)) {
      if (focusAfter) {
        openSummaryPanel(nodeId);
      }
      return;
    }

    loadingNodeIds.add(nodeId);
    cy.getElementById(nodeId).addClass('loading');

    try {
      const payload = await api.get<GraphResponse>(`/api/v1/graph/network/${nodeId}?radius=1`);
      const newNodeIds = mergePayload(payload, nodeId);
      applyLayout(newNodeIds);
      refreshNodeLabels();
      refreshEdgeVisibility();
      expandedNodeIds.add(nodeId);

      if (focusAfter) {
        openSummaryPanel(nodeId);
      }
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to expand node';
    } finally {
      loadingNodeIds.delete(nodeId);
      cy.getElementById(nodeId).removeClass('loading');
    }
  }

  async function expandViewportBoundary(): Promise<void> {
    if (!cy) {
      return;
    }

    const ext = cy.extent();
    const margin = Math.min(ext.w, ext.h) * 0.25;

    const nearBoundary = cy
      .nodes()
      .map((node) => {
        const position = node.position();
        const isNear =
          position.x <= ext.x1 + margin ||
          position.x >= ext.x2 - margin ||
          position.y <= ext.y1 + margin ||
          position.y >= ext.y2 - margin;
        return { id: node.id(), isNear, type: String(node.data('type') ?? '') };
      })
      .filter((entry) => entry.isNear && entry.type === 'person')
      .map((entry) => entry.id)
      .filter((id) => !expandedNodeIds.has(id) && !loadingNodeIds.has(id))
      .slice(0, 1);

    for (const nodeId of nearBoundary) {
      await expandFromNode(nodeId, false);
    }
  }

  function collapseSubtree(rootId: string): void {
    if (!cy) {
      return;
    }

    const queue = [rootId];
    const visited = new Set<string>([rootId]);

    while (queue.length > 0) {
      const current = queue.shift() ?? '';
      if (!current) {
        continue;
      }

      cy.edges().forEach((edge) => {
        const edgeType = String(edge.data('edge_type') ?? '');
        const source = String(edge.data('source') ?? '');
        const target = String(edge.data('target') ?? '');
        if (edgeType !== 'parent_of' || source !== current) {
          return;
        }

        if (!visited.has(target)) {
          visited.add(target);
          queue.push(target);
          if (target !== rootId) {
            hiddenNodeIds.add(target);
          }
        }
      });
    }

    refreshEdgeVisibility();
    contextMenu = { open: false, nodeId: '', x: 0, y: 0 };
  }

  function clearCollapsed(): void {
    hiddenNodeIds.clear();
    refreshEdgeVisibility();
  }

  function onColorModeChange(): void {
    refreshNodeLabels();
    if (!cy) {
      return;
    }
    cy.nodes().forEach((node) => {
      node.data('color_mode', colorMode);
    });
    cy.style()
      .selector('node')
      .style({ 'background-color': styleForNode() })
      .update();
  }

  function onLayoutModeChange(): void {
    applyLayout(new Set<string>());
  }

  onMount(async () => {
    await initializeGraphCanvas();
    await ensureInitialCenter();
  });

  onDestroy(() => {
    if (searchDebounce) {
      clearTimeout(searchDebounce);
    }
    if (viewportDebounce) {
      clearTimeout(viewportDebounce);
    }
    if (clickDebounce) {
      clearTimeout(clickDebounce);
    }
    if (layoutDebounce) {
      clearTimeout(layoutDebounce);
    }

    if (cy) {
      cy.destroy();
      cy = null;
    }
  });
</script>

<main class="panel">
  <header class="header">
    <h1>Relationship Graph</h1>
    <p>Cytoscape network explorer with async neighbourhood expansion.</p>
  </header>

  <section class="guide" aria-label="How to read relationship graph">
    <h2>How to follow relationships (including divorce cases)</h2>
    <p>
      When tracing ancestry, prioritize <strong>parent_of / child_of edges</strong>. Partner edges
      (including divorced/separated histories) describe household/union context but do not change
      who the biological/adoptive parents are in lineage traversal.
    </p>
    <ul>
      <li><strong>Parent/child edges</strong>: use these to move up/down generations.</li>
      <li><strong>Partner edges</strong>: relationship context only (married/divorced/etc.).</li>
      <li><strong>Hide event edges</strong> can reduce noise while following tree structure.</li>
    </ul>
  </section>

  <section class="controls">
    <div class="search-area">
      <label for="person-search">Root person</label>
      <input
        id="person-search"
        type="search"
        bind:value={searchInput}
        on:input={onSearchInput}
        placeholder="Search person to centre graph…"
      />
      {#if searchResults.length > 0}
        <ul class="search-results">
          {#each searchResults as result}
            <li>
              <button type="button" on:click={() => chooseCenter(result)}>
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

    <div class="control-row">
      <label>
        Layout
        <select bind:value={layoutMode} on:change={onLayoutModeChange}>
          <option value="cose-bilkent">cose-bilkent</option>
          <option value="cola">cola</option>
          <option value="breadthfirst">breadthfirst</option>
          <option value="circle">circle</option>
        </select>
      </label>

      <label>
        Node colours
        <select bind:value={colorMode} on:change={onColorModeChange}>
          <option value="confidence">Confidence gradient</option>
          <option value="gender">Gender mode</option>
        </select>
      </label>

      <label class="checkbox">
        <input type="checkbox" bind:checked={hideEventEdges} on:change={refreshEdgeVisibility} />
        Hide event edges
      </label>

      <button type="button" on:click={() => cy?.fit(undefined, 55)}>Fit to screen</button>
      <button type="button" class="ghost" on:click={clearCollapsed}>Reset collapsed</button>
    </div>
  </section>

  {#if selectedCenterId}
    <p class="center-indicator">
      Centre: <strong>{selectedCenterName || selectedCenterId}</strong>
    </p>
  {/if}

  {#if loading}
    <p class="status">Loading graph…</p>
  {/if}

  {#if error}
    <p class="error">{error}</p>
  {/if}

  <div class="graph-wrap">
    <div bind:this={graphContainer} class="graph-canvas"></div>

    {#if edgeTooltip.open}
      <div class="edge-tooltip" style={`left:${edgeTooltip.x + 10}px;top:${edgeTooltip.y + 10}px;`}>
        {edgeTooltip.text}
      </div>
    {/if}

    {#if contextMenu.open}
      <div class="context-menu" style={`left:${contextMenu.x}px;top:${contextMenu.y}px;`}>
        <button type="button" on:click={() => goto(`/persons/${contextMenu.nodeId}`)}>
          Navigate to detail
        </button>
        <button
          type="button"
          on:click={() => {
            const summary = nodesById.get(contextMenu.nodeId);
            if (summary) {
              void chooseCenter({
                entity_type: 'person',
                entity_id: contextMenu.nodeId,
                display_name: summary.label,
                snippet: null
              });
            }
            contextMenu = { open: false, nodeId: '', x: 0, y: 0 };
          }}
        >
          Set as centre
        </button>
        <button
          type="button"
          on:click={() => {
            void expandFromNode(contextMenu.nodeId, true);
            contextMenu = { open: false, nodeId: '', x: 0, y: 0 };
          }}
        >
          Expand
        </button>
        <button type="button" on:click={() => collapseSubtree(contextMenu.nodeId)}>
          Collapse subtree
        </button>
      </div>
    {/if}
  </div>
</main>

{#if sidePanelOpen && selectedNodeSummary}
  <button type="button" class="overlay" aria-label="Close node summary" on:click={() => (sidePanelOpen = false)}></button>
  <aside class="side-panel">
    <h2>{selectedNodeSummary.label}</h2>
    <p><strong>Life:</strong> {lifeLabel(selectedNodeSummary)}</p>
    <p><strong>Node type:</strong> {selectedNodeSummary.type}</p>
    <p><strong>Node id:</strong> <code>{selectedNodeSummary.id}</code></p>
    <div class="panel-actions">
      <button type="button" on:click={() => goto(`/persons/${selectedNodeSummary?.id}`)}>
        Open person detail
      </button>
      <button type="button" class="ghost" on:click={() => selectedNodeSummary && expandFromNode(selectedNodeSummary.id, true)}>
        Expand neighbourhood
      </button>
      <button
        type="button"
        class="ghost"
        on:click={() => {
          if (!selectedNodeSummary) {
            return;
          }

          void chooseCenter({
            entity_type: 'person',
            entity_id: selectedNodeSummary.id,
            display_name: selectedNodeSummary.label,
            snippet: null
          });
        }}
      >
        Set as centre
      </button>
    </div>
  </aside>
{/if}

<style>
  .panel {
    background: #ffffff;
    border: 1px solid #e2e8f0;
    border-radius: 0.75rem;
    padding: 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
    position: relative;
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
    gap: 0.7rem;
  }

  .search-area {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    max-width: 32rem;
  }

  .search-area label {
    font-size: 0.875rem;
    color: #334155;
  }

  .search-area input,
  select {
    border: 1px solid #cbd5e1;
    border-radius: 0.45rem;
    padding: 0.45rem 0.55rem;
    font: inherit;
    background: #ffffff;
  }

  .search-results {
    position: absolute;
    z-index: 30;
    top: calc(100% + 0.25rem);
    left: 0;
    right: 0;
    margin: 0;
    padding: 0.35rem;
    list-style: none;
    border: 1px solid #e2e8f0;
    background: #ffffff;
    border-radius: 0.45rem;
    box-shadow: 0 8px 24px rgb(15 23 42 / 16%);
  }

  .search-results button {
    width: 100%;
    border: 0;
    background: transparent;
    text-align: left;
    padding: 0.45rem 0.4rem;
    border-radius: 0.35rem;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .search-results button:hover {
    background: #f8fafc;
  }

  .search-results small {
    color: #64748b;
    font-size: 0.78rem;
  }

  .control-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.6rem;
  }

  .control-row label {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    color: #334155;
    font-size: 0.9rem;
  }

  .checkbox {
    gap: 0.5rem;
  }

  .control-row button {
    border: 0;
    border-radius: 0.45rem;
    padding: 0.4rem 0.7rem;
    background: #2563eb;
    color: #ffffff;
    cursor: pointer;
  }

  .control-row .ghost {
    background: #f8fafc;
    color: #0f172a;
    border: 1px solid #cbd5e1;
  }

  .center-indicator,
  .status {
    margin: 0;
    color: #334155;
  }

  .error {
    margin: 0;
    color: #b91c1c;
  }

  .graph-wrap {
    position: relative;
    min-height: 68vh;
    border: 1px solid #e2e8f0;
    border-radius: 0.6rem;
    overflow: hidden;
    background: #f8fafc;
  }

  .graph-canvas {
    width: 100%;
    height: 68vh;
  }

  .edge-tooltip {
    position: absolute;
    z-index: 25;
    pointer-events: none;
    background: #0f172a;
    color: #ffffff;
    border-radius: 0.35rem;
    padding: 0.35rem 0.45rem;
    font-size: 0.78rem;
    max-width: 20rem;
  }

  .context-menu {
    position: absolute;
    z-index: 26;
    background: #ffffff;
    border: 1px solid #e2e8f0;
    border-radius: 0.45rem;
    box-shadow: 0 10px 24px rgb(15 23 42 / 18%);
    min-width: 12rem;
    overflow: hidden;
  }

  .context-menu button {
    width: 100%;
    border: 0;
    background: #ffffff;
    text-align: left;
    padding: 0.5rem 0.6rem;
    cursor: pointer;
    color: #0f172a;
  }

  .context-menu button:hover {
    background: #f8fafc;
  }

  .overlay {
    position: fixed;
    inset: 0;
    border: 0;
    width: 100%;
    padding: 0;
    border-radius: 0;
    background: rgb(15 23 42 / 35%);
  }

  .side-panel {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(460px, 100%);
    background: #ffffff;
    border-left: 1px solid #e2e8f0;
    padding: 1rem;
    overflow: auto;
    z-index: 35;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .side-panel h2,
  .side-panel p {
    margin: 0;
  }

  .panel-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-top: 0.4rem;
  }

  .panel-actions button {
    border: 0;
    border-radius: 0.45rem;
    padding: 0.45rem 0.7rem;
    cursor: pointer;
    background: #2563eb;
    color: #ffffff;
  }

  .panel-actions .ghost {
    border: 1px solid #cbd5e1;
    background: #ffffff;
    color: #0f172a;
  }
</style>
