import { describe, expect, it, vi } from 'vitest';

import {
  createCitationForAssertion,
  runSourceDrivenCreateEvent,
  runSourceDrivenCreatePerson,
  type ApiClient
} from '$lib/sourceDrivenWorkflow';

function createMockClient(): ApiClient {
  return {
    get: vi.fn(async (path: string) => {
      if (path === '/api/v1/persons/person-created/assertions') {
        return {
          name: [{ assertion_id: 'person-assertion-1' }]
        };
      }
      return {};
    }),
    post: vi.fn(async (path: string) => {
      if (path === '/api/v1/persons') {
        return { id: 'person-created' };
      }
      if (path === '/api/v1/events') {
        return { id: 'event-created' };
      }
      if (path === '/api/v1/events/event-created/assertions') {
        return { assertion_id: 'event-assertion-1' };
      }
      return { id: 'ok' };
    }),
    put: vi.fn(async () => ({ id: 'ok' }))
  };
}

const citation = {
  sourceId: 'source-1',
  page: '42',
  folio: '',
  entry: '',
  citationNote: 'line 8',
  confidenceLevel: 2,
  transcription: 'entry text',
  dateAccessed: '2026-04-02'
};

describe('sourceDrivenWorkflow integration', () => {
  it('creates person and links selected citation to generated name assertion', async () => {
    const client = createMockClient();

    await runSourceDrivenCreatePerson(client, citation, {
      givenNames: 'John',
      surnames: 'Adams',
      gender: 'Unknown'
    });

    expect(client.post).toHaveBeenCalledWith('/api/v1/persons', expect.any(Object));
    expect(client.get).toHaveBeenCalledWith('/api/v1/persons/person-created/assertions');
    expect(client.post).toHaveBeenCalledWith(
      '/api/v1/citations',
      expect.objectContaining({
        source_id: 'source-1',
        assertion_id: 'person-assertion-1'
      })
    );
  });

  it('creates event and links selected citation to generated event assertion', async () => {
    const client = createMockClient();

    await runSourceDrivenCreateEvent(client, citation, {
      eventType: 'Birth',
      description: 'Birth entry',
      personId: ''
    });

    expect(client.post).toHaveBeenCalledWith('/api/v1/events', expect.any(Object));
    expect(client.post).toHaveBeenCalledWith('/api/v1/events/event-created/assertions', expect.any(Object));
    expect(client.post).toHaveBeenCalledWith(
      '/api/v1/citations',
      expect.objectContaining({
        source_id: 'source-1',
        assertion_id: 'event-assertion-1'
      })
    );
  });

  it('creates citation payload with date accessed textual value', async () => {
    const client = createMockClient();

    await createCitationForAssertion(client, 'assertion-1', citation);

    expect(client.post).toHaveBeenCalledWith(
      '/api/v1/citations',
      expect.objectContaining({
        assertion_id: 'assertion-1',
        date_accessed: {
          Textual: {
            text: '2026-04-02'
          }
        }
      })
    );
  });
});
