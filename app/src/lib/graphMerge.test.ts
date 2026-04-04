import { describe, expect, it } from 'vitest';

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

type MinimalCyElement = {
  length: number;
};

type AddCall = {
  nodes: Array<{ data: Record<string, unknown> }>;
  edges: Array<{ data: Record<string, unknown> }>;
};

function edgeIdFor(edge: GraphEdge): string {
  return `${edge.source}|${edge.target}|${edge.edge_type}|${edge.label}`;
}

function mergePayloadForTest(
  cy: { getElementById: (id: string) => MinimalCyElement; add: (elements: AddCall) => void },
  payload: GraphResponse,
  edgeIds: Set<string>
): Set<string> {
  const nodesToAdd: Array<{ data: Record<string, unknown> }> = [];
  const edgesToAdd: Array<{ data: Record<string, unknown> }> = [];
  const newNodeIds = new Set<string>();

  for (const node of payload.nodes) {
    const exists = cy.getElementById(node.id);
    if (exists.length === 0) {
      nodesToAdd.push({
        data: {
          id: node.id,
          label: node.label,
          type: node.type,
          birth_year: node.birth_year,
          death_year: node.death_year
        }
      });
      newNodeIds.add(node.id);
    }
  }

  for (const edge of payload.edges) {
    const id = edgeIdFor(edge);
    if (edgeIds.has(id)) {
      continue;
    }
    edgeIds.add(id);
    edgesToAdd.push({
      data: {
        id,
        source: edge.source,
        target: edge.target,
        label: edge.label,
        edge_type: edge.edge_type
      }
    });
  }

  if (nodesToAdd.length > 0) {
    cy.add({ nodes: nodesToAdd, edges: [] });
  }
  if (edgesToAdd.length > 0) {
    cy.add({ nodes: [], edges: edgesToAdd });
  }

  return newNodeIds;
}

describe('relationship graph merge payload behavior', () => {
  it('adds missing nodes before edges and deduplicates existing elements', () => {
    const existing = new Set<string>(['existing-person']);
    const calls: AddCall[] = [];

    const cy = {
      getElementById: (id: string): MinimalCyElement => ({ length: existing.has(id) ? 1 : 0 }),
      add: (elements: AddCall) => {
        calls.push(elements);
        for (const node of elements.nodes) {
          existing.add(String(node.data.id));
        }
      }
    };

    const edgeIds = new Set<string>();

    const payload: GraphResponse = {
      nodes: [
        {
          id: 'existing-person',
          label: 'Existing Person',
          type: 'person',
          birth_year: null,
          death_year: null
        },
        {
          id: 'family-1',
          label: 'Family',
          type: 'family',
          birth_year: null,
          death_year: null
        },
        {
          id: 'child-1',
          label: 'Child',
          type: 'person',
          birth_year: 1900,
          death_year: 1980
        }
      ],
      edges: [
        {
          source: 'existing-person',
          target: 'family-1',
          label: 'child_of',
          edge_type: 'child_of'
        },
        {
          source: 'family-1',
          target: 'child-1',
          label: 'parent_of',
          edge_type: 'parent_of'
        }
      ]
    };

    const firstNewNodeIds = mergePayloadForTest(cy, payload, edgeIds);

    expect(firstNewNodeIds).toEqual(new Set(['family-1', 'child-1']));
    expect(calls.length).toBe(2);
    expect(calls[0].nodes.map((node) => node.data.id)).toEqual(['family-1', 'child-1']);
    expect(calls[0].edges).toEqual([]);
    expect(calls[1].nodes).toEqual([]);
    expect(calls[1].edges).toHaveLength(2);

    calls.length = 0;
    const secondNewNodeIds = mergePayloadForTest(cy, payload, edgeIds);
    expect(secondNewNodeIds.size).toBe(0);
    expect(calls.length).toBe(0);
  });
});
