use crate::types::{DateValue, EntityId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RepositoryType {
    #[default]
    Archive,
    Library,
    Website,
    PersonalCollection,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    pub id: EntityId,
    pub name: String,
    pub repository_type: RepositoryType,
    pub address: Option<String>,
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub _raw_gedcom: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryRef {
    pub repository_id: EntityId,
    pub call_number: Option<String>,
    pub media_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub id: EntityId,
    pub title: String,
    pub author: Option<String>,
    pub publication_info: Option<String>,
    pub abbreviation: Option<String>,
    #[serde(default)]
    pub repository_refs: Vec<RepositoryRef>,
    #[serde(default)]
    pub _raw_gedcom: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    pub id: EntityId,
    pub source_id: EntityId,
    pub volume: Option<String>,
    pub page: Option<String>,
    pub folio: Option<String>,
    pub entry: Option<String>,
    pub confidence_level: Option<u8>,
    pub date_accessed: Option<DateValue>,
    pub transcription: Option<String>,
    #[serde(default)]
    pub _raw_gedcom: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CitationRef {
    pub citation_id: EntityId,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DimensionsPx {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DimensionsMm {
    pub width_mm: u32,
    pub height_mm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CropRectPct {
    /// Left offset as percentage [0..100]
    pub x_pct: u8,
    /// Top offset as percentage [0..100]
    pub y_pct: u8,
    /// Width as percentage [0..100]
    pub width_pct: u8,
    /// Height as percentage [0..100]
    pub height_pct: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Media {
    pub id: EntityId,
    pub file_path: String,
    pub content_hash: String,
    pub mime_type: String,
    pub thumbnail_path: Option<String>,
    pub ocr_text: Option<String>,
    pub dimensions_px: Option<DimensionsPx>,
    pub physical_dimensions_mm: Option<DimensionsMm>,
    pub caption: Option<String>,
    #[serde(default)]
    pub _raw_gedcom: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaRef {
    pub media_id: EntityId,
    pub crop_rect_pct: Option<CropRectPct>,
    pub caption: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoteType {
    #[default]
    General,
    Research,
    Transcript,
    SourceText,
    Todo,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: EntityId,
    pub text: String,
    pub note_type: NoteType,
    #[serde(default)]
    pub _raw_gedcom: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteRef {
    pub note_id: EntityId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Calendar, FuzzyDate};

    #[test]
    fn serde_round_trip_repository_source_citation_chain() {
        let repository = Repository {
            id: EntityId::new(),
            name: "The National Archives".to_string(),
            repository_type: RepositoryType::Archive,
            address: Some("Kew, London".to_string()),
            urls: vec!["https://www.nationalarchives.gov.uk".to_string()],
            _raw_gedcom: BTreeMap::new(),
        };

        let source = Source {
            id: EntityId::new(),
            title: "1881 England Census".to_string(),
            author: Some("Registrar General".to_string()),
            publication_info: Some("Public Record Office".to_string()),
            abbreviation: Some("1881 Census".to_string()),
            repository_refs: vec![RepositoryRef {
                repository_id: repository.id,
                call_number: Some("RG11".to_string()),
                media_type: Some("Microfilm".to_string()),
            }],
            _raw_gedcom: BTreeMap::new(),
        };

        let citation = Citation {
            id: EntityId::new(),
            source_id: source.id,
            volume: Some("13".to_string()),
            page: Some("42".to_string()),
            folio: Some("7".to_string()),
            entry: Some("17".to_string()),
            confidence_level: Some(3),
            date_accessed: Some(DateValue::Exact {
                date: FuzzyDate::new(2026, Some(3), Some(30)),
                calendar: Calendar::Gregorian,
            }),
            transcription: Some("John Doe, Head, age 41".to_string()),
            _raw_gedcom: BTreeMap::new(),
        };

        let json_repo = serde_json::to_string(&repository).expect("serialize repository");
        let json_source = serde_json::to_string(&source).expect("serialize source");
        let json_citation = serde_json::to_string(&citation).expect("serialize citation");

        let round_repo: Repository =
            serde_json::from_str(&json_repo).expect("deserialize repository");
        let round_source: Source = serde_json::from_str(&json_source).expect("deserialize source");
        let round_citation: Citation =
            serde_json::from_str(&json_citation).expect("deserialize citation");

        assert_eq!(round_repo, repository);
        assert_eq!(round_source, source);
        assert_eq!(round_citation, citation);
    }

    #[test]
    fn serde_round_trip_media_and_note_refs() {
        let media = Media {
            id: EntityId::new(),
            file_path: "/tmp/census-1881.jpg".to_string(),
            content_hash: "sha256:abc123".to_string(),
            mime_type: "image/jpeg".to_string(),
            thumbnail_path: Some("/tmp/thumbs/census-1881.jpg".to_string()),
            ocr_text: Some("Household schedule".to_string()),
            dimensions_px: Some(DimensionsPx {
                width: 2048,
                height: 1536,
            }),
            physical_dimensions_mm: Some(DimensionsMm {
                width_mm: 210,
                height_mm: 297,
            }),
            caption: Some("1881 Census page".to_string()),
            _raw_gedcom: BTreeMap::new(),
        };

        let media_ref = MediaRef {
            media_id: media.id,
            crop_rect_pct: Some(CropRectPct {
                x_pct: 10,
                y_pct: 20,
                width_pct: 30,
                height_pct: 40,
            }),
            caption: Some("John Doe entry".to_string()),
        };

        let note = Note {
            id: EntityId::new(),
            text: "Possible transcription ambiguity in surname.".to_string(),
            note_type: NoteType::Research,
            _raw_gedcom: BTreeMap::new(),
        };

        let note_ref = NoteRef { note_id: note.id };
        let citation_ref = CitationRef {
            citation_id: EntityId::new(),
            note: Some("Linked to census household".to_string()),
        };

        let json_media = serde_json::to_string(&media).expect("serialize media");
        let json_media_ref = serde_json::to_string(&media_ref).expect("serialize media_ref");
        let json_note = serde_json::to_string(&note).expect("serialize note");
        let json_note_ref = serde_json::to_string(&note_ref).expect("serialize note_ref");
        let json_citation_ref =
            serde_json::to_string(&citation_ref).expect("serialize citation_ref");

        let round_media: Media = serde_json::from_str(&json_media).expect("deserialize media");
        let round_media_ref: MediaRef =
            serde_json::from_str(&json_media_ref).expect("deserialize media_ref");
        let round_note: Note = serde_json::from_str(&json_note).expect("deserialize note");
        let round_note_ref: NoteRef =
            serde_json::from_str(&json_note_ref).expect("deserialize note_ref");
        let round_citation_ref: CitationRef =
            serde_json::from_str(&json_citation_ref).expect("deserialize citation_ref");

        assert_eq!(round_media, media);
        assert_eq!(round_media_ref, media_ref);
        assert_eq!(round_note, note);
        assert_eq!(round_note_ref, note_ref);
        assert_eq!(round_citation_ref, citation_ref);
    }
}
