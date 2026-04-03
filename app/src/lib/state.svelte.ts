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
  activeSandboxId: null as string | null,
  pendingRequests: 0
});

function persistRecentItems(): void {
  if (typeof window === 'undefined') {
    return;
  }

  localStorage.setItem('recent_items', JSON.stringify(appState.recentItems));
}

function persistSandboxState(): void {
  if (typeof window === 'undefined') {
    return;
  }

  localStorage.setItem('sandbox_mode', JSON.stringify(appState.sandboxMode));
  if (appState.activeSandboxId) {
    localStorage.setItem('active_sandbox_id', appState.activeSandboxId);
  } else {
    localStorage.removeItem('active_sandbox_id');
  }
}

export function setCurrentView(view: string): void {
  appState.currentView = view;
}

export function setSandboxMode(enabled: boolean, sandboxId?: string): void {
  appState.sandboxMode = enabled;
  if (enabled && sandboxId) {
    appState.activeSandboxId = sandboxId;
  } else if (!enabled) {
    appState.activeSandboxId = null;
  }
  persistSandboxState();
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

export function restoreSandboxState(): void {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    const sandboxModeRaw = localStorage.getItem('sandbox_mode');
    if (sandboxModeRaw !== null) {
      appState.sandboxMode = JSON.parse(sandboxModeRaw) as boolean;
    }

    if (appState.sandboxMode) {
      const sandboxId = localStorage.getItem('active_sandbox_id');
      if (sandboxId) {
        appState.activeSandboxId = sandboxId;
      }
    }
  } catch {
    // Reset to defaults if parsing fails
    appState.sandboxMode = false;
    appState.activeSandboxId = null;
  }
}