<script lang="ts">
  import { goto } from '$app/navigation';

  type RelatedNode = {
    id: string;
    label: string;
    href: string;
    kind: 'person' | 'family' | 'event' | 'source' | 'citation' | 'repository' | 'media' | 'other';
  };

  export let centerLabel = '';
  export let nodes: RelatedNode[] = [];

  const width = 520;
  const height = 260;
  const centerX = 260;
  const centerY = 130;
  const radius = 95;

  function shortLabel(label: string): string {
    return label.length > 18 ? `${label.slice(0, 18)}…` : label;
  }

  function kindColor(kind: RelatedNode['kind']): string {
    switch (kind) {
      case 'person': return '#8b5cf6';
      case 'family': return '#0ea5e9';
      case 'event': return '#10b981';
      case 'source': return '#f59e0b';
      case 'citation': return '#ec4899';
      case 'repository': return '#6366f1';
      case 'media': return '#14b8a6';
      default: return '#64748b';
    }
  }

  $: positioned = nodes.map((node, index) => {
    const angle = (index / Math.max(nodes.length, 1)) * Math.PI * 2 - Math.PI / 2;
    return {
      ...node,
      x: centerX + radius * Math.cos(angle),
      y: centerY + radius * Math.sin(angle)
    };
  });
</script>

<section class="related-graph">
  <header>
    <h3>Related records graph</h3>
    <span class="count">{nodes.length}</span>
  </header>

  {#if nodes.length === 0}
    <p class="empty">No related records available yet.</p>
  {:else}
    <svg viewBox={`0 0 ${width} ${height}`} role="img" aria-label="Related records graph">
      {#each positioned as node}
        <line x1={centerX} y1={centerY} x2={node.x} y2={node.y} stroke="#ddd6fe" stroke-width="1.5" />
      {/each}

      <circle cx={centerX} cy={centerY} r="27" fill="#ede9fe" stroke="#7c3aed" stroke-width="2" />
      <text x={centerX} y={centerY + 4} text-anchor="middle" class="center-label">{shortLabel(centerLabel)}</text>

      {#each positioned as node}
        <g>
          <circle cx={node.x} cy={node.y} r="18" fill="#fff" stroke={kindColor(node.kind)} stroke-width="2" />
          <text x={node.x} y={node.y + 4} text-anchor="middle" class="node-dot">•</text>
        </g>
      {/each}
    </svg>

    <ul class="node-links">
      {#each nodes as node}
        <li>
          <button type="button" class="node-link" on:click={() => goto(node.href)}>
            <span class="kind" style={`background:${kindColor(node.kind)}`}></span>
            <strong>{node.kind}</strong>
            <span>{node.label}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .related-graph {
    border: 1px solid #e5def8;
    border-radius: 0.75rem;
    padding: 0.8rem;
    background: #fff;
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  h3 {
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

  svg {
    width: 100%;
    border: 1px solid #f0e8ff;
    border-radius: 0.6rem;
    background: #fffdff;
  }

  .center-label {
    font-size: 0.68rem;
    fill: #4c1d95;
    font-weight: 600;
  }

  .node-dot {
    font-size: 0.85rem;
    fill: #7c3aed;
  }

  .node-links {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 0.35rem;
  }

  .node-link {
    width: 100%;
    border: 1px solid #efe6ff;
    border-radius: 0.45rem;
    background: #fffdff;
    cursor: pointer;
    font: inherit;
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.35rem 0.45rem;
    text-align: left;
  }

  .node-link:hover {
    background: #faf5ff;
  }

  .kind {
    width: 0.55rem;
    height: 0.55rem;
    border-radius: 999px;
    flex: 0 0 auto;
  }
</style>
