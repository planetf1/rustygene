use crate::types::DateValue;
use chrono::{Duration, NaiveDate};

pub const DEFAULT_MIN_PARENT_AGE_GAP_YEARS: i64 = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    ImpossibleDate {
        field: &'static str,
        message: String,
    },
    BirthAfterDeath,
    ParentTooYoung {
        min_years: i64,
    },
    SelfParentage,
    EventOutsideLifespan,
}

#[derive(Debug, Clone, Copy)]
struct DateBounds {
    lower: Option<NaiveDate>,
    upper: Option<NaiveDate>,
}

#[must_use]
fn fuzzy_bounds(date: crate::types::FuzzyDate) -> (NaiveDate, NaiveDate) {
    (date.to_sortable_date(true), date.to_sortable_date(false))
}

fn bounds(date: &DateValue) -> Option<DateBounds> {
    let b = match date {
        DateValue::Exact { date, .. } => {
            let (start, end) = fuzzy_bounds(*date);
            DateBounds {
                lower: Some(start),
                upper: Some(end),
            }
        }
        DateValue::Range { from, to, .. } => {
            let (start, _) = fuzzy_bounds(*from);
            let (_, end) = fuzzy_bounds(*to);
            DateBounds {
                lower: Some(start),
                upper: Some(end),
            }
        }
        DateValue::Before { date, .. } => {
            let (_, end) = fuzzy_bounds(*date);
            DateBounds {
                lower: None,
                upper: Some(end),
            }
        }
        DateValue::After { date, .. } => {
            let (start, _) = fuzzy_bounds(*date);
            DateBounds {
                lower: Some(start),
                upper: None,
            }
        }
        DateValue::About { date, .. } => {
            let (start, end) = fuzzy_bounds(*date);
            DateBounds {
                lower: Some(start),
                upper: Some(end),
            }
        }
        DateValue::Tolerance {
            date,
            plus_minus_days,
            ..
        } => {
            let center = date.to_sortable_date(true);
            let delta = Duration::days(i64::from(*plus_minus_days));
            DateBounds {
                lower: Some(center - delta),
                upper: Some(center + delta),
            }
        }
        DateValue::Quarter { year, quarter } => {
            if !(1..=4).contains(quarter) {
                return None;
            }
            let start_month = (u32::from(*quarter) - 1) * 3 + 1;
            let end_month = start_month + 2;
            let start = NaiveDate::from_ymd_opt(*year, start_month, 1)?;
            let end_day = match end_month {
                4 | 6 | 9 | 11 => 30,
                2 => {
                    if *year % 4 == 0 && (*year % 100 != 0 || *year % 400 == 0) {
                        29
                    } else {
                        28
                    }
                }
                _ => 31,
            };
            let end = NaiveDate::from_ymd_opt(*year, end_month, end_day)?;
            DateBounds {
                lower: Some(start),
                upper: Some(end),
            }
        }
        DateValue::Textual { .. } => DateBounds {
            lower: None,
            upper: None,
        },
    };

    Some(b)
}

pub fn validate_date_possible(date: &DateValue) -> Result<(), ValidationError> {
    match date {
        DateValue::Exact { date, .. }
        | DateValue::Before { date, .. }
        | DateValue::After { date, .. }
        | DateValue::About { date, .. }
        | DateValue::Tolerance { date, .. } => {
            if bounds(&DateValue::Exact {
                date: *date,
                calendar: crate::types::Calendar::Gregorian,
            })
            .is_none()
            {
                return Err(ValidationError::ImpossibleDate {
                    field: "date",
                    message: "invalid day/month combination".to_string(),
                });
            }
        }
        DateValue::Range { from, to, .. } => {
            let from_bounds = bounds(&DateValue::Exact {
                date: *from,
                calendar: crate::types::Calendar::Gregorian,
            })
            .ok_or_else(|| ValidationError::ImpossibleDate {
                field: "range.from",
                message: "invalid start date".to_string(),
            })?;

            let to_bounds = bounds(&DateValue::Exact {
                date: *to,
                calendar: crate::types::Calendar::Gregorian,
            })
            .ok_or_else(|| ValidationError::ImpossibleDate {
                field: "range.to",
                message: "invalid end date".to_string(),
            })?;

            if from_bounds.lower > to_bounds.upper {
                return Err(ValidationError::ImpossibleDate {
                    field: "range",
                    message: "range start is after range end".to_string(),
                });
            }
        }
        DateValue::Quarter { quarter, .. } => {
            if !(1..=4).contains(quarter) {
                return Err(ValidationError::ImpossibleDate {
                    field: "quarter",
                    message: "quarter must be 1..=4".to_string(),
                });
            }
        }
        DateValue::Textual { .. } => {}
    }

    Ok(())
}

pub fn validate_birth_before_death(
    birth_date: &DateValue,
    death_date: &DateValue,
) -> Result<(), ValidationError> {
    validate_date_possible(birth_date)?;
    validate_date_possible(death_date)?;

    let birth = bounds(birth_date).ok_or_else(|| ValidationError::ImpossibleDate {
        field: "birth_date",
        message: "could not derive sortable bounds".to_string(),
    })?;
    let death = bounds(death_date).ok_or_else(|| ValidationError::ImpossibleDate {
        field: "death_date",
        message: "could not derive sortable bounds".to_string(),
    })?;

    if let (Some(birth_latest), Some(death_earliest)) = (birth.upper, death.lower)
        && birth_latest > death_earliest
    {
        return Err(ValidationError::BirthAfterDeath);
    }

    Ok(())
}

pub fn validate_parent_age_gap(
    parent_birth_date: &DateValue,
    child_birth_date: &DateValue,
    min_parent_age_gap_years: i64,
) -> Result<(), ValidationError> {
    validate_date_possible(parent_birth_date)?;
    validate_date_possible(child_birth_date)?;

    let parent = bounds(parent_birth_date).ok_or_else(|| ValidationError::ImpossibleDate {
        field: "parent_birth_date",
        message: "could not derive sortable bounds".to_string(),
    })?;
    let child = bounds(child_birth_date).ok_or_else(|| ValidationError::ImpossibleDate {
        field: "child_birth_date",
        message: "could not derive sortable bounds".to_string(),
    })?;

    if let (Some(parent_latest), Some(child_earliest)) = (parent.upper, child.lower) {
        let min_gap = Duration::days(min_parent_age_gap_years.saturating_mul(365));
        if child_earliest < parent_latest + min_gap {
            return Err(ValidationError::ParentTooYoung {
                min_years: min_parent_age_gap_years,
            });
        }
    }

    Ok(())
}

pub fn validate_no_self_parentage(
    parent_id: crate::types::EntityId,
    child_id: crate::types::EntityId,
) -> Result<(), ValidationError> {
    if parent_id == child_id {
        return Err(ValidationError::SelfParentage);
    }

    Ok(())
}

pub fn validate_event_within_lifespan(
    event_date: &DateValue,
    birth_date: Option<&DateValue>,
    death_date: Option<&DateValue>,
) -> Result<(), ValidationError> {
    validate_date_possible(event_date)?;

    let event_bounds = bounds(event_date).ok_or_else(|| ValidationError::ImpossibleDate {
        field: "event_date",
        message: "could not derive sortable bounds".to_string(),
    })?;

    if let Some(birth) = birth_date {
        validate_date_possible(birth)?;
        let birth_bounds = bounds(birth).ok_or_else(|| ValidationError::ImpossibleDate {
            field: "birth_date",
            message: "could not derive sortable bounds".to_string(),
        })?;

        if let (Some(event_latest), Some(birth_earliest)) = (event_bounds.upper, birth_bounds.lower)
            && event_latest < birth_earliest
        {
            return Err(ValidationError::EventOutsideLifespan);
        }
    }

    if let Some(death) = death_date {
        validate_date_possible(death)?;
        let death_bounds = bounds(death).ok_or_else(|| ValidationError::ImpossibleDate {
            field: "death_date",
            message: "could not derive sortable bounds".to_string(),
        })?;

        if let (Some(event_earliest), Some(death_latest)) = (event_bounds.lower, death_bounds.upper)
            && event_earliest > death_latest
        {
            return Err(ValidationError::EventOutsideLifespan);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Calendar, EntityId, FuzzyDate};

    fn exact(year: i32, month: u8, day: u8) -> DateValue {
        DateValue::Exact {
            date: FuzzyDate::new(year, Some(month), Some(day)),
            calendar: Calendar::Gregorian,
        }
    }

    #[test]
    fn validates_birth_before_death() {
        let birth = exact(1850, 5, 1);
        let death = exact(1900, 1, 1);
        assert!(validate_birth_before_death(&birth, &death).is_ok());
    }

    #[test]
    fn rejects_birth_after_death() {
        let birth = exact(1901, 1, 1);
        let death = exact(1900, 1, 1);
        assert_eq!(
            validate_birth_before_death(&birth, &death),
            Err(ValidationError::BirthAfterDeath)
        );
    }

    #[test]
    fn validates_parent_age_gap() {
        let parent = exact(1840, 1, 1);
        let child = exact(1860, 1, 1);
        assert!(
            validate_parent_age_gap(&parent, &child, DEFAULT_MIN_PARENT_AGE_GAP_YEARS).is_ok()
        );
    }

    #[test]
    fn rejects_parent_too_young() {
        let parent = exact(1855, 1, 1);
        let child = exact(1860, 1, 1);
        assert_eq!(
            validate_parent_age_gap(&parent, &child, DEFAULT_MIN_PARENT_AGE_GAP_YEARS),
            Err(ValidationError::ParentTooYoung {
                min_years: DEFAULT_MIN_PARENT_AGE_GAP_YEARS
            })
        );
    }

    #[test]
    fn validates_no_self_parentage() {
        let parent = EntityId::new();
        let child = EntityId::new();
        assert!(validate_no_self_parentage(parent, child).is_ok());
    }

    #[test]
    fn rejects_self_parentage() {
        let person = EntityId::new();
        assert_eq!(
            validate_no_self_parentage(person, person),
            Err(ValidationError::SelfParentage)
        );
    }

    #[test]
    fn validates_event_within_lifespan() {
        let birth = exact(1850, 1, 1);
        let death = exact(1910, 1, 1);
        let event = exact(1881, 4, 3);

        assert!(validate_event_within_lifespan(&event, Some(&birth), Some(&death)).is_ok());
    }

    #[test]
    fn rejects_event_after_death() {
        let birth = exact(1850, 1, 1);
        let death = exact(1910, 1, 1);
        let event = exact(1920, 1, 1);

        assert_eq!(
            validate_event_within_lifespan(&event, Some(&birth), Some(&death)),
            Err(ValidationError::EventOutsideLifespan)
        );
    }

    #[test]
    fn rejects_event_before_birth() {
        let birth = exact(1850, 1, 1);
        let event = exact(1849, 12, 31);

        assert_eq!(
            validate_event_within_lifespan(&event, Some(&birth), None),
            Err(ValidationError::EventOutsideLifespan)
        );
    }

    #[test]
    fn rejects_invalid_range_date() {
        let invalid = DateValue::Range {
            from: FuzzyDate::new(1900, Some(1), Some(1)),
            to: FuzzyDate::new(1899, Some(12), Some(31)),
            calendar: Calendar::Gregorian,
        };

        assert_eq!(
            validate_date_possible(&invalid),
            Err(ValidationError::ImpossibleDate {
                field: "range",
                message: "range start is after range end".to_string(),
            })
        );
    }
}
