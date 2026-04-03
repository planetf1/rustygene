import { describe, expect, it } from 'vitest';

import {
  detectImportFormat,
  formatLabel,
  mergeUniqueLogMessages,
  normalizeEntityCounts,
  reportCards
} from '$lib/importWizard';

describe('importWizard helpers', () => {
  it('detects supported formats from file extension', () => {
    expect(detectImportFormat('kennedy.ged')).toBe('gedcom');
    expect(detectImportFormat('family.gramps')).toBe('gramps_xml');
    expect(detectImportFormat('family.xml')).toBe('gramps_xml');
    expect(detectImportFormat('snapshot.json')).toBe('json');
    expect(detectImportFormat('notes.txt')).toBeNull();
  });

  it('merges log messages without duplicates', () => {
    expect(
      mergeUniqueLogMessages(['Import queued.', 'Parsing GEDCOM file...'], [
        'Parsing GEDCOM file...',
        'Import completed.'
      ])
    ).toEqual(['Import queued.', 'Parsing GEDCOM file...', 'Import completed.']);
  });

  it('normalizes missing entity counts to zero and preserves extras', () => {
    expect(normalizeEntityCounts({ person: 3, source: 2, repository: 1 })).toEqual({
      person: 3,
      family: 0,
      event: 0,
      source: 2,
      media: 0,
      note: 0,
      repository: 1
    });
  });

  it('builds report cards in stable order', () => {
    expect(reportCards({ person: 12, family: 5, event: 17, source: 2, media: 1, note: 4 })).toEqual([
      { key: 'person', label: 'Persons imported', value: 12 },
      { key: 'family', label: 'Families', value: 5 },
      { key: 'event', label: 'Events', value: 17 },
      { key: 'source', label: 'Sources', value: 2 },
      { key: 'media', label: 'Media references', value: 1 },
      { key: 'note', label: 'Notes', value: 4 }
    ]);
    expect(formatLabel('gedcom')).toBe('GEDCOM 5.5.1');
  });
});