export type NameType = 'Birth' | 'Married' | 'Aka' | 'Nickname' | 'Other';
export type Gender = 'Male' | 'Female' | 'Other' | 'Unknown';
export type SurnameOrigin = 'Patronymic' | 'Matronymic' | 'Toponymic' | 'Occupational' | 'Unknown';

export type CitationDraft = {
  sourceId: string;
  volume: string;
  page: string;
  folio: string;
  entry: string;
  confidenceLevel: number | null;
  transcription: string;
  citationNote: string;
};

export type PersonDraft = {
  id?: string;
  givenNames: string[];
  surnames: { value: string; originType: SurnameOrigin; connector: string }[];
  nameType: NameType;
  sortAs: string;
  callName: string;
  gender: Gender;
  birthDate: string;
  birthPlace: string;
  deathDate: string;
  deathPlace: string;
  notes: string;
  citations: CitationDraft[];
};

export type ParticipantDraft = {
  personId: string;
  role: string;
};

export type EventDraft = {
  id?: string;
  eventType: string;
  date: string;
  placeId: string;
  description: string;
  participants: ParticipantDraft[];
  citations: CitationDraft[];
};

export type PartnerLink = 'Married' | 'Unmarried' | 'Unknown';

export type FamilyDraft = {
  id?: string;
  partner1Id: string;
  partner2Id: string;
  childIds: string[];
  partnerLink: PartnerLink;
  marriageDate: string;
  marriagePlace: string;
  notes: string;
};
