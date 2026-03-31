use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Base identifier for all domain entities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EntityId(pub Uuid);

impl EntityId {
    #[must_use]
    pub fn new() -> Self {
        EntityId(Uuid::new_v4())
    }
}

impl Default for EntityId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Calendar system for historical accuracy.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Calendar {
    #[default]
    Gregorian,
    Julian,
    DualDate,
    Hebrew,
    FrenchRepublican,
    Islamic,
}

/// A flexible date structure supporting exact, year-month, or year-only values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FuzzyDate {
    pub year: i32,
    pub month: Option<u8>,
    pub day: Option<u8>,
}

impl FuzzyDate {
    #[must_use]
    pub fn new(year: i32, month: Option<u8>, day: Option<u8>) -> Self {
        Self { year, month, day }
    }

    /// Converts the fuzzy date into a regular NaiveDate for sorting boundaries.
    /// If `start` is true, returns the earliest possible date (e.g., Jan 1).
    /// If `start` is false, returns the latest possible date (e.g., Dec 31).
    #[must_use]
    pub fn to_sortable_date(&self, start: bool) -> NaiveDate {
        let m = self.month.unwrap_or(if start { 1 } else { 12 });
        let d = self.day.unwrap_or({
            if start {
                1
            } else {
                match m {
                    4 | 6 | 9 | 11 => 30,
                    2 => {
                        if self.year % 4 == 0 && (self.year % 100 != 0 || self.year % 400 == 0) {
                            29
                        } else {
                            28
                        }
                    }
                    _ => 31,
                }
            }
        });

        NaiveDate::from_ymd_opt(self.year, u32::from(m), u32::from(d))
            .unwrap_or_else(|| NaiveDate::from_ymd_opt(self.year, 1, 1).unwrap())
    }
}

/// Represents the chronological center point of a DateValue.
fn date_center(dv: &DateValue) -> Option<NaiveDate> {
    match dv {
        DateValue::Exact { date, .. }
        | DateValue::Before { date, .. }
        | DateValue::After { date, .. }
        | DateValue::About { date, .. }
        | DateValue::Tolerance { date, .. } => Some(date.to_sortable_date(true)),
        DateValue::Range { from, to, .. } => {
            let start = from.to_sortable_date(true);
            let end = to.to_sortable_date(false);
            let num_days = (end - start).num_days();
            Some(start + Duration::days(num_days / 2))
        }
        DateValue::Quarter { year, quarter } => {
            let m = u32::from(*quarter.clamp(&1, &4) - 1) * 3 + 2; // Middle month of quarter
            Some(NaiveDate::from_ymd_opt(*year, m, 15).unwrap())
        }
        DateValue::Textual { .. } => None,
    }
}

/// A value asserting a historical date, ranging from exact to textual.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DateValue {
    Exact {
        date: FuzzyDate,
        calendar: Calendar,
    },
    Range {
        from: FuzzyDate,
        to: FuzzyDate,
        calendar: Calendar,
    },
    Before {
        date: FuzzyDate,
        calendar: Calendar,
    },
    After {
        date: FuzzyDate,
        calendar: Calendar,
    },
    About {
        date: FuzzyDate,
        calendar: Calendar,
    },
    Tolerance {
        date: FuzzyDate,
        plus_minus_days: u32,
        calendar: Calendar,
    },
    Quarter {
        year: i32,
        quarter: u8,
    },
    Textual {
        value: String,
    },
}

impl PartialOrd for DateValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let left = date_center(self)?;
        let right = date_center(other)?;
        left.partial_cmp(&right)
    }
}

/// Standard representation of gender.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Gender {
    Male,
    Female,
    Unknown,
    Custom(String),
}

/// Represents an actor (user, agent, or import job) responsible for an assertion.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ActorRef {
    User(String),
    Agent(String),
    Import(String),
}

impl fmt::Display for ActorRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActorRef::User(id) => write!(f, "user:{}", id),
            ActorRef::Agent(name) => write!(f, "agent:{}", name),
            ActorRef::Import(job) => write!(f, "import:{}", job),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorRefParseError(pub String);

impl fmt::Display for ActorRefParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid ActorRef format: {}", self.0)
    }
}

impl FromStr for ActorRef {
    type Err = ActorRefParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(ActorRefParseError(s.to_string()));
        }

        match parts[0] {
            "user" => Ok(ActorRef::User(parts[1].to_string())),
            "agent" => Ok(ActorRef::Agent(parts[1].to_string())),
            "import" => Ok(ActorRef::Import(parts[1].to_string())),
            _ => Err(ActorRefParseError(s.to_string())),
        }
    }
}

impl Serialize for ActorRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ActorRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        ActorRef::from_str(&s).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_date_value_ordering() {
        let d1 = DateValue::Exact {
            date: FuzzyDate::new(1850, Some(5), Some(1)),
            calendar: Calendar::Gregorian,
        };
        let d2 = DateValue::Exact {
            date: FuzzyDate::new(1850, Some(10), Some(1)),
            calendar: Calendar::Gregorian,
        };
        assert!(d1 < d2);

        let d_fuzzy = DateValue::About {
            date: FuzzyDate::new(1850, None, None),
            calendar: Calendar::Gregorian,
        };
        // 1850-01-01 < 1850-05-01
        assert!(d_fuzzy < d1);

        let d3 = DateValue::Textual {
            value: "some weird string".to_string(),
        };
        assert_eq!(d1.partial_cmp(&d3), None);
    }

    #[test]
    fn test_actor_ref_serialization() {
        let actor = ActorRef::Agent("doc-processor".to_string());
        let json = serde_json::to_string(&actor).unwrap();
        assert_eq!(json, "\"agent:doc-processor\"");

        let deserialized: ActorRef = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, actor);
    }

    #[test]
    fn test_date_value_serialization() {
        let dv = DateValue::Exact {
            date: FuzzyDate::new(1850, Some(5), Some(1)),
            calendar: Calendar::Gregorian,
        };
        let json = serde_json::to_string(&dv).unwrap();
        let expected =
            r#"{"type":"Exact","date":{"year":1850,"month":5,"day":1},"calendar":"gregorian"}"#;
        assert_eq!(json, expected);

        let deserialized: DateValue = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, dv);
    }
}
