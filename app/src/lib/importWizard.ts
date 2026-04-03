export type ImportFormat = 'gedcom' | 'gramps_xml' | 'json';

export type ImportJobState = 'queued' | 'running' | 'completed' | 'failed';

export type ImportWarningDetail = {
  code: string;
  title: string;
  counts: Record<string, number>;
};

export type ImportJobStatus = {
  job_id: string;
  status: ImportJobState;
  progress_pct: number;
  entities_imported: number | null;
  entities_imported_by_type?: Record<string, number> | null;
  errors: string[];
  warnings: string[];
  warning_details?: ImportWarningDetail[];
  log_messages?: string[];
  completed_at: string | null;
};

export type ImportCompletedEvent = {
  event: 'import.completed';
  job_id: string;
  entities_imported: Record<string, number>;
  timestamp: string;
};

const DEFAULT_ENTITY_ORDER = ['person', 'family', 'event', 'source', 'media', 'note'];

export function detectImportFormat(fileName: string): ImportFormat | null {
  const normalized = fileName.trim().toLowerCase();
  if (normalized.endsWith('.ged')) {
    return 'gedcom';
  }
  if (normalized.endsWith('.gramps') || normalized.endsWith('.xml')) {
    return 'gramps_xml';
  }
  if (normalized.endsWith('.json')) {
    return 'json';
  }
  return null;
}

export function formatLabel(format: ImportFormat): string {
  switch (format) {
    case 'gedcom':
      return 'GEDCOM 5.5.1';
    case 'gramps_xml':
      return 'Gramps XML';
    case 'json':
      return 'JSON';
  }
}

export function mergeUniqueLogMessages(existing: string[], incoming: string[]): string[] {
  const seen = new Set(existing);
  const next = [...existing];

  for (const message of incoming) {
    const trimmed = message.trim();
    if (!trimmed || seen.has(trimmed)) {
      continue;
    }
    seen.add(trimmed);
    next.push(trimmed);
  }

  return next;
}

export function normalizeEntityCounts(counts?: Record<string, number> | null): Record<string, number> {
  const normalized: Record<string, number> = {};

  for (const key of DEFAULT_ENTITY_ORDER) {
    normalized[key] = counts?.[key] ?? 0;
  }

  if (!counts) {
    return normalized;
  }

  for (const [key, value] of Object.entries(counts)) {
    if (!(key in normalized)) {
      normalized[key] = value;
    }
  }

  return normalized;
}

export function reportCards(counts?: Record<string, number> | null): Array<{ key: string; label: string; value: number }> {
  const normalized = normalizeEntityCounts(counts);

  return [
    { key: 'person', label: 'Persons imported', value: normalized.person ?? 0 },
    { key: 'family', label: 'Families', value: normalized.family ?? 0 },
    { key: 'event', label: 'Events', value: normalized.event ?? 0 },
    { key: 'source', label: 'Sources', value: normalized.source ?? 0 },
    { key: 'media', label: 'Media references', value: normalized.media ?? 0 },
    { key: 'note', label: 'Notes', value: normalized.note ?? 0 }
  ];
}