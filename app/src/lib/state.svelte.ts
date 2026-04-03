export type EntityType = 'person' | 'family' | 'event' | 'source' | 'repository';

export type RecentItem = {
  entityType: EntityType;
  id: string;
  displayName: string;
  visitedAt: string;
};

export const appState = $state({
  isLoading: false,
  apiPort: null as number | null,
  currentView: '/',
  recentItems: [] as RecentItem[],
  sandboxMode: false,
  pendingRequests: 0
});

function persistRecentItems(): void {
  if (typeof window === 'undefined') {
    return;
  }

  localStorage.setItem('recent_items', JSON.stringify(appState.recentItems));
}

export function setCurrentView(view: string): void {
  appState.currentView = view;
}

export function setSandboxMode(enabled: boolean): void {
  appState.sandboxMode = enabled;
}

export function incrementPendingRequests(): void {
  appState.pendingRequests += 1;
  appState.isLoading = appState.pendingRequests > 0;
}

export function decrementPendingRequests(): void {
  appState.pendingRequests = Math.max(0, appState.pendingRequests - 1);
  appState.isLoading = appState.pendingRequests > 0;
}

export function addRecentItem(item: Omit<RecentItem, 'visitedAt'>): void {
  const next: RecentItem = {
    ...item,
    visitedAt: new Date().toISOString()
  };

  const deduped = appState.recentItems.filter(
    (existing) => !(existing.entityType === item.entityType && existing.id === item.id)
  );

  appState.recentItems = [next, ...deduped].slice(0, 20);
  persistRecentItems();
}

export function restoreRecentItems(): void {
  if (typeof window === 'undefined') {
    return;
  }

  const raw = localStorage.getItem('recent_items');
  if (!raw) {
    appState.recentItems = [];
    return;
  }

  try {
    const parsed = JSON.parse(raw) as RecentItem[];
    if (!Array.isArray(parsed)) {
      appState.recentItems = [];
      return;
    }

    appState.recentItems = parsed.slice(0, 20);
  } catch {
    appState.recentItems = [];
  }
}