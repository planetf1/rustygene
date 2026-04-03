export type ApiClient = {
  get: <T>(path: string) => Promise<T>;
  post: <T>(path: string, body: unknown) => Promise<T>;
  put: <T>(path: string, body: unknown) => Promise<T>;
};

export type CitationInput = {
  sourceId: string;
  page: string;
  folio: string;
  entry: string;
  citationNote: string;
  confidenceLevel: number | null;
  transcription: string;
  dateAccessed: string;
};

type AssertionRow = { assertion_id: string };

function dateAccessedPayload(dateAccessed: string): { Textual: { text: string } } | null {
  const text = dateAccessed.trim();
  if (!text) {
    return null;
  }

  return {
    Textual: {
      text
    }
  };
}

export async function createCitationForAssertion(
  client: ApiClient,
  assertionId: string,
  citation: CitationInput
): Promise<void> {
  await client.post('/api/v1/citations', {
    source_id: citation.sourceId,
    assertion_id: assertionId,
    citation_note: citation.citationNote.trim() || null,
    volume: null,
    page: citation.page.trim() || null,
    folio: citation.folio.trim() || null,
    entry: citation.entry.trim() || null,
    confidence_level: citation.confidenceLevel,
    date_accessed: dateAccessedPayload(citation.dateAccessed),
    transcription: citation.transcription.trim() || null
  });
}

export async function runSourceDrivenCreatePerson(
  client: ApiClient,
  citation: CitationInput,
  person: {
    givenNames: string;
    surnames: string;
    gender: string;
  }
): Promise<string> {
  const givenNames = person.givenNames
    .split(',')
    .map((value) => value.trim())
    .filter((value) => value.length > 0);
  const surnames = person.surnames
    .split(',')
    .map((value) => value.trim())
    .filter((value) => value.length > 0)
    .map((value) => ({ value, origin_type: 'Unknown', connector: null }));

  const created = await client.post<{ id: string }>('/api/v1/persons', {
    given_names: givenNames.length ? givenNames : ['Unknown'],
    surnames: surnames.length ? surnames : [{ value: 'Unknown', origin_type: 'Unknown', connector: null }],
    name_type: 'Birth',
    gender: person.gender,
    sort_as: null,
    call_name: null,
    prefix: null,
    suffix: null,
    birth_date: null,
    birth_place: null
  });

  const grouped = await client.get<Record<string, AssertionRow[]>>(`/api/v1/persons/${created.id}/assertions`);
  const assertionId = grouped.name?.[0]?.assertion_id;
  if (!assertionId) {
    throw new Error('Unable to locate created person assertion for citation linking.');
  }

  await createCitationForAssertion(client, assertionId, citation);
  return created.id;
}

export async function runSourceDrivenCreateEvent(
  client: ApiClient,
  citation: CitationInput,
  event: {
    eventType: string;
    description: string;
    personId: string;
  }
): Promise<string> {
  const created = await client.post<{ id: string }>('/api/v1/events', {
    event_type: event.eventType,
    date: null,
    place_id: null,
    description: event.description.trim() || null
  });

  if (event.personId) {
    await client.post(`/api/v1/events/${created.id}/participants`, {
      person_id: event.personId,
      role: 'Principal'
    });
  }

  const createdAssertion = await client.post<{ assertion_id: string }>(`/api/v1/events/${created.id}/assertions`, {
    field: 'description',
    value: event.description.trim() || `Source-driven ${event.eventType} assertion`,
    confidence: 0.8,
    status: 'proposed',
    source_citations: []
  });

  await createCitationForAssertion(client, createdAssertion.assertion_id, citation);
  return created.id;
}
