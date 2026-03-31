use crate::types::{DateValue, EntityId, Gender};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NameType {
    #[default]
    Birth,
    Married,
    Aka,
    Immigrant,
    Religious,
    Custom(String),
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SurnameOrigin {
    #[default]
    Patrilineal,
    Matrilineal,
    Patronymic,
    Matronymic,
    Location,
    Occupation,
    Feudal,
    Pseudonym,
    Taken,
    Inherited,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Surname {
    pub value: String,
    pub origin_type: SurnameOrigin,
    /// e.g. "van der", "y" for compound parts
    pub connector: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonName {
    pub name_type: NameType,
    pub date_range: Option<DateValue>,
    /// First and middle names, e.g. "John Paul"
    pub given_names: String,
    pub call_name: Option<String>,
    pub surnames: Vec<Surname>,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    /// Override for cultural sorting (e.g. patronymic names sorted by first name)
    pub sort_as: Option<String>,
}

impl PersonName {
    /// Generates a standardized sort key for this name.
    /// Connectors are ignored by default. "van der Bilt" sorts under "B".
    /// If `sort_as` is present, it returns that.
    #[must_use]
    pub fn sort_key(&self) -> String {
        if let Some(ref override_key) = self.sort_as {
            return override_key.clone();
        }

        let mut parts = Vec::new();
        for s in &self.surnames {
            parts.push(s.value.clone());
        }

        let mut key = parts.join(" ");
        if !key.is_empty() && !self.given_names.is_empty() {
            key.push_str(", ");
        }
        key.push_str(&self.given_names);

        key.to_lowercase()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub id: EntityId,
    pub names: Vec<PersonName>,
    pub gender: Gender,
    pub living: bool,
    pub private: bool,
    /// Original GEDCOM xref ID (e.g., "@I23@") for round-trip preservation
    #[serde(default)]
    pub original_xref: Option<String>,
    /// Escape hatch for unstructured vendor-specific tags
    #[serde(default)]
    pub _raw_gedcom: BTreeMap<String, String>,
}

impl Person {
    /// Returns the primary name of the person.
    /// Tries to find a `Birth` name first, falls back to the 0th index,
    /// and generates an "Unknown" name if the list is empty.
    #[must_use]
    pub fn primary_name(&self) -> PersonName {
        if self.names.is_empty() {
            return PersonName {
                given_names: "Unknown".to_string(),
                ..Default::default()
            };
        }

        self.names
            .iter()
            .find(|n| n.name_type == NameType::Birth)
            .unwrap_or(&self.names[0])
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sort_key_standard() {
        let name = PersonName {
            given_names: "John".to_string(),
            surnames: vec![Surname {
                value: "Smith".to_string(),
                origin_type: SurnameOrigin::Patrilineal,
                connector: None,
            }],
            ..Default::default()
        };
        assert_eq!(name.sort_key(), "smith, john");
    }

    #[test]
    fn test_sort_key_connector() {
        let name = PersonName {
            given_names: "Vincent".to_string(),
            surnames: vec![Surname {
                value: "Gogh".to_string(),
                origin_type: SurnameOrigin::Patrilineal,
                connector: Some("van".to_string()),
            }],
            ..Default::default()
        };
        assert_eq!(name.sort_key(), "gogh, vincent"); // Notice "van" is not here due to ADR-001
    }

    #[test]
    fn test_sort_key_override() {
        let name = PersonName {
            given_names: "Jón".to_string(),
            surnames: vec![Surname {
                value: "Stefánsson".to_string(),
                origin_type: SurnameOrigin::Patronymic,
                connector: None,
            }],
            sort_as: Some("Jón Stefánsson".to_string()),
            ..Default::default()
        };
        // The override maintains exact casing and structure of the inputted override string
        assert_eq!(name.sort_key(), "Jón Stefánsson");
    }

    #[test]
    fn test_primary_name() {
        let p = Person {
            id: EntityId::new(),
            names: vec![
                PersonName {
                    name_type: NameType::Married,
                    given_names: "Jane".to_string(),
                    surnames: vec![Surname {
                        value: "Doe".to_string(),
                        origin_type: SurnameOrigin::Patrilineal,
                        connector: None,
                    }],
                    ..Default::default()
                },
                PersonName {
                    name_type: NameType::Birth,
                    given_names: "Jane".to_string(),
                    surnames: vec![Surname {
                        value: "Smith".to_string(),
                        origin_type: SurnameOrigin::Patrilineal,
                        connector: None,
                    }],
                    ..Default::default()
                },
            ],
            gender: Gender::Female,
            living: false,
            private: false,
            original_xref: Some("@I42@".to_string()),
            _raw_gedcom: BTreeMap::new(),
        };

        let primary = p.primary_name();
        assert_eq!(primary.name_type, NameType::Birth);
        assert_eq!(primary.surnames[0].value, "Smith");
    }
}
