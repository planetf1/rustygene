import { writable } from 'svelte/store';

import { api } from '$lib/api';

export type AncestorApiNode = {
  person_id: string;
  display_name: string;
  birth_year: number | null;
  death_year: number | null;
  confidence: number;
  father: AncestorApiNode | null;
  mother: AncestorApiNode | null;
};

type AncestorCacheState = {
  cache: Map<string, AncestorApiNode>;
};

const state = writable<AncestorCacheState>({
  cache: new Map<string, AncestorApiNode>()
});

function cacheKey(personId: string, generations: number): string {
  return `${personId}::${generations}`;
}

export function cloneAncestor(node: AncestorApiNode): AncestorApiNode {
  return {
    person_id: node.person_id,
    display_name: node.display_name,
    birth_year: node.birth_year,
    death_year: node.death_year,
    confidence: node.confidence,
    father: node.father ? cloneAncestor(node.father) : null,
    mother: node.mother ? cloneAncestor(node.mother) : null
  };
}

async function fetchAncestors(
  personId: string,
  generations: number,
  forceRefresh = false
): Promise<AncestorApiNode> {
  const key = cacheKey(personId, generations);

  if (!forceRefresh) {
    let cached: AncestorApiNode | undefined;
    state.update((current) => {
      cached = current.cache.get(key);
      return current;
    });

    if (cached) {
      return cloneAncestor(cached);
    }
  }

  const payload = await api.get<AncestorApiNode>(
    `/api/v1/graph/ancestors/${personId}?generations=${generations}`
  );

  state.update((current) => {
    const next = new Map(current.cache);
    next.set(key, cloneAncestor(payload));
    return { cache: next };
  });

  return cloneAncestor(payload);
}

function clear(): void {
  state.set({ cache: new Map<string, AncestorApiNode>() });
}

export const ancestorDataStore = {
  subscribe: state.subscribe,
  fetchAncestors,
  clear
};
