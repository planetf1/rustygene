import { describe, expect, it, beforeEach } from 'vitest';
import { addRecentItem, appState, restoreRecentItems, setCurrentView, setSandboxMode } from './state.svelte';

describe('state restore smoke', () => {
  beforeEach(() => {
    const storage = new Map<string, string>();
    (globalThis as unknown as { localStorage: Storage }).localStorage = {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => {
        storage.set(key, value);
      },
      removeItem: (key: string) => {
        storage.delete(key);
      },
      clear: () => {
        storage.clear();
      },
      key: (index: number) => Array.from(storage.keys())[index] ?? null,
      get length() {
        return storage.size;
      }
    } as Storage;

    localStorage.clear();
    appState.recentItems = [];
    appState.currentView = '/';
    appState.sandboxMode = false;
  });

  it('restores recent items and route state from localStorage', () => {
    addRecentItem({
      entityType: 'person',
      id: 'person-1',
      displayName: 'Test Person'
    });

    const persisted = localStorage.getItem('recent_items');
    expect(persisted).toBeTruthy();

    appState.recentItems = [];
    restoreRecentItems();

    expect(appState.recentItems.length).toBe(1);
    expect(appState.recentItems[0]?.id).toBe('person-1');

    setCurrentView('/persons/person-1');
    expect(appState.currentView).toBe('/persons/person-1');

    setSandboxMode(true);
    expect(appState.sandboxMode).toBe(true);
  });
});
