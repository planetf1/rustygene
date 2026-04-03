use std::cmp::Ordering;
use std::collections::BTreeSet;

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use rustygene_core::event::{Event, EventType};
use rustygene_core::person::Person;
use rustygene_core::types::{DateValue, EntityId};
use rustygene_storage::Pagination;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::ApiError;
use crate::AppState;

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(default, rename = "type")]
    entity_type: Option<String>,
    #[serde(default)]
    strategy: Option<String>,
    #[serde(default)]
    date_from: Option<String>,
    #[serde(default)]
    date_to: Option<String>,
    #[serde(default)]
    place: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchStrategy {
    Exact,
    Fts,
    Phonetic,
    Combined,
}

impl SearchStrategy {
    fn parse(raw: Option<&str>) -> Result<Self, ApiError> {
        match raw.unwrap_or("combined").to_ascii_lowercase().as_str() {
            "exact" => Ok(Self::Exact),
            "fts" => Ok(Self::Fts),
            "phonetic" => Ok(Self::Phonetic),
            "combined" => Ok(Self::Combined),
            other => Err(ApiError::BadRequest(format!(
                "invalid strategy: {other} (expected exact|fts|phonetic|combined)"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Fts => "fts",
            Self::Phonetic => "phonetic",
            Self::Combined => "combined",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntityFilter {
    Person,
    Family,
    Event,
    Source,
    Place,
    Note,
}

impl EntityFilter {
    fn parse(raw: Option<&str>) -> Result<Option<Self>, ApiError> {
        let Some(value) = raw else {
            return Ok(None);
        };

        match value.to_ascii_lowercase().as_str() {
            "person" => Ok(Some(Self::Person)),
            "family" => Ok(Some(Self::Family)),
            "event" => Ok(Some(Self::Event)),
            "source" => Ok(Some(Self::Source)),
            "place" => Ok(Some(Self::Place)),
            "note" => Ok(Some(Self::Note)),
            other => Err(ApiError::BadRequest(format!(
                "invalid type: {other} (expected person|family|event|source|place|note)"
            ))),
        }
    }

    fn as_storage_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Family => "family",
            Self::Event => "event",
            Self::Source => "source",
            Self::Place => "place",
            Self::Note => "note",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct SearchResultResponse {
    entity_type: String,
    entity_id: Uuid,
    display_name: String,
    match_fields: Vec<String>,
    score: f32,
    snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct SearchResponse {
    query: String,
    strategy_used: String,
    results: Vec<SearchResultResponse>,
    total: usize,
}

#[derive(Debug, Clone)]
struct RawMatch {
    entity_id: EntityId,
    entity_type: String,
    score: f32,
    snippet: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(search))
}

async fn search(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, ApiError> {
    let q = query.q.trim();
    if q.is_empty() {
        return Err(ApiError::BadRequest(
            "query parameter q is required".to_string(),
        ));
    }

    let strategy = SearchStrategy::parse(query.strategy.as_deref())?;
    let entity_filter = EntityFilter::parse(query.entity_type.as_deref())?;
    let place_filter = query
        .place
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase);

    let date_from_year = query.date_from.as_deref().map(parse_year).transpose()?;
    let date_to_year = query.date_to.as_deref().map(parse_year).transpose()?;

    let limit = query.limit.unwrap_or(20).min(100) as usize;
    let offset = query.offset.unwrap_or(0) as usize;

    let Some(sqlite) = state.sqlite_backend.clone() else {
        return Err(ApiError::InternalError(
            "sqlite backend not available for search".to_string(),
        ));
    };

    let fts_query = build_fts_query(q)?;
    let phonetic_query = build_phonetic_query(q);

    let mut rows = match strategy {
        SearchStrategy::Exact => run_exact_query(&sqlite, q, entity_filter)?,
        SearchStrategy::Fts => run_fts_query(&sqlite, &fts_query, entity_filter)?,
        SearchStrategy::Phonetic => {
            if phonetic_query.is_empty() {
                Vec::new()
            } else {
                run_fts_query(&sqlite, &phonetic_query, entity_filter)?
            }
        }
        SearchStrategy::Combined => {
            let mut primary = run_fts_query(&sqlite, &fts_query, entity_filter)?;
            if primary.len() < 3 && !phonetic_query.is_empty() {
                let secondary = run_fts_query(&sqlite, &phonetic_query, entity_filter)?;
                merge_matches(&mut primary, secondary);
            }
            primary
        }
    };

    let should_include_person =
        entity_filter.is_none() || entity_filter == Some(EntityFilter::Person);
    if should_include_person {
        let fallback_rows = fallback_person_matches(&state, q, strategy).await?;
        merge_matches(&mut rows, fallback_rows);
    }

    rows.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
    });

    let mut filtered_results = Vec::new();
    for row in rows {
        if !passes_post_filters(
            &state,
            row.entity_id,
            &row.entity_type,
            place_filter.as_deref(),
            date_from_year,
            date_to_year,
        )
        .await?
        {
            continue;
        }

        if let Some(result) = enrich_row(&state, row).await? {
            filtered_results.push(result);
        }
    }

    let total = filtered_results.len();
    let results = filtered_results
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();

    Ok(Json(SearchResponse {
        query: q.to_string(),
        strategy_used: strategy.as_str().to_string(),
        results,
        total,
    }))
}

fn run_fts_query(
    sqlite: &std::sync::Arc<rustygene_storage::sqlite_impl::SqliteBackend>,
    fts_query: &str,
    entity_filter: Option<EntityFilter>,
) -> Result<Vec<RawMatch>, ApiError> {
    sqlite.with_connection(|conn| {
        let sql = if entity_filter.is_some() {
            "SELECT entity_id, entity_type, bm25(search_index) AS rank, snippet(search_index, 2, '<b>', '</b>', ' … ', 8) AS snip
             FROM search_index
             WHERE search_index MATCH ? AND entity_type = ?"
        } else {
            "SELECT entity_id, entity_type, bm25(search_index) AS rank, snippet(search_index, 2, '<b>', '</b>', ' … ', 8) AS snip
             FROM search_index
             WHERE search_index MATCH ?"
        };

        let mut stmt = conn.prepare(sql).map_err(|e| rustygene_storage::StorageError {
            code: rustygene_storage::StorageErrorCode::Backend,
            message: format!("prepare search query failed: {e}"),
        })?;

        if let Some(filter) = entity_filter {
            let rows = stmt
                .query_map(rusqlite::params![fts_query, filter.as_storage_str()], |row| {
                    let id_text: String = row.get(0)?;
                    let parsed_uuid = Uuid::parse_str(&id_text).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;

                    Ok(RawMatch {
                        entity_id: EntityId(parsed_uuid),
                        entity_type: row.get::<_, String>(1)?,
                        score: row.get::<_, f64>(2).unwrap_or(0.0).abs() as f32,
                        snippet: row.get::<_, Option<String>>(3)?,
                    })
                })
                .map_err(|e| rustygene_storage::StorageError {
                    code: rustygene_storage::StorageErrorCode::Backend,
                    message: format!("execute search query failed: {e}"),
                })?;

            rows.collect::<Result<Vec<_>, _>>().map_err(|e| rustygene_storage::StorageError {
                code: rustygene_storage::StorageErrorCode::Backend,
                message: format!("collect search results failed: {e}"),
            })
        } else {
            let rows = stmt
                .query_map(rusqlite::params![fts_query], |row| {
                    let id_text: String = row.get(0)?;
                    let parsed_uuid = Uuid::parse_str(&id_text).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;

                    Ok(RawMatch {
                        entity_id: EntityId(parsed_uuid),
                        entity_type: row.get::<_, String>(1)?,
                        score: row.get::<_, f64>(2).unwrap_or(0.0).abs() as f32,
                        snippet: row.get::<_, Option<String>>(3)?,
                    })
                })
                .map_err(|e| rustygene_storage::StorageError {
                    code: rustygene_storage::StorageErrorCode::Backend,
                    message: format!("execute search query failed: {e}"),
                })?;

            rows.collect::<Result<Vec<_>, _>>().map_err(|e| rustygene_storage::StorageError {
                code: rustygene_storage::StorageErrorCode::Backend,
                message: format!("collect search results failed: {e}"),
            })
        }
    })
    .map_err(ApiError::from)
}

fn run_exact_query(
    sqlite: &std::sync::Arc<rustygene_storage::sqlite_impl::SqliteBackend>,
    query: &str,
    entity_filter: Option<EntityFilter>,
) -> Result<Vec<RawMatch>, ApiError> {
    let needle = format!("%{}%", query.to_ascii_lowercase());
    sqlite
        .with_connection(|conn| {
            let sql = if entity_filter.is_some() {
                "SELECT entity_id, entity_type
             FROM search_index
             WHERE content LIKE ? AND entity_type = ?"
            } else {
                "SELECT entity_id, entity_type
             FROM search_index
             WHERE content LIKE ?"
            };

            let mut stmt = conn
                .prepare(sql)
                .map_err(|e| rustygene_storage::StorageError {
                    code: rustygene_storage::StorageErrorCode::Backend,
                    message: format!("prepare exact query failed: {e}"),
                })?;

            if let Some(filter) = entity_filter {
                let rows = stmt
                    .query_map(rusqlite::params![needle, filter.as_storage_str()], |row| {
                        let id_text: String = row.get(0)?;
                        let parsed_uuid = Uuid::parse_str(&id_text).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?;

                        Ok(RawMatch {
                            entity_id: EntityId(parsed_uuid),
                            entity_type: row.get::<_, String>(1)?,
                            score: 1.0,
                            snippet: None,
                        })
                    })
                    .map_err(|e| rustygene_storage::StorageError {
                        code: rustygene_storage::StorageErrorCode::Backend,
                        message: format!("execute exact query failed: {e}"),
                    })?;

                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(|e| rustygene_storage::StorageError {
                        code: rustygene_storage::StorageErrorCode::Backend,
                        message: format!("collect exact results failed: {e}"),
                    })
            } else {
                let rows = stmt
                    .query_map(rusqlite::params![needle], |row| {
                        let id_text: String = row.get(0)?;
                        let parsed_uuid = Uuid::parse_str(&id_text).map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?;

                        Ok(RawMatch {
                            entity_id: EntityId(parsed_uuid),
                            entity_type: row.get::<_, String>(1)?,
                            score: 1.0,
                            snippet: None,
                        })
                    })
                    .map_err(|e| rustygene_storage::StorageError {
                        code: rustygene_storage::StorageErrorCode::Backend,
                        message: format!("execute exact query failed: {e}"),
                    })?;

                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(|e| rustygene_storage::StorageError {
                        code: rustygene_storage::StorageErrorCode::Backend,
                        message: format!("collect exact results failed: {e}"),
                    })
            }
        })
        .map_err(ApiError::from)
}

fn merge_matches(primary: &mut Vec<RawMatch>, secondary: Vec<RawMatch>) {
    let mut seen = BTreeSet::new();
    for existing in primary.iter() {
        seen.insert((existing.entity_type.clone(), existing.entity_id));
    }

    for row in secondary {
        let key = (row.entity_type.clone(), row.entity_id);
        if !seen.contains(&key) {
            primary.push(row);
            seen.insert(key);
        }
    }
}

async fn enrich_row(
    state: &AppState,
    row: RawMatch,
) -> Result<Option<SearchResultResponse>, ApiError> {
    let entity_type = row.entity_type.to_ascii_lowercase();
    let display_name = match entity_type.as_str() {
        "person" => match state.storage.get_person(row.entity_id).await {
            Ok(person) => person_display_name(&person),
            Err(_) => return Ok(None),
        },
        "place" => match state.storage.get_place(row.entity_id).await {
            Ok(place) => place
                .names
                .first()
                .map(|name| name.name.clone())
                .unwrap_or_else(|| format!("Place {}", row.entity_id)),
            Err(_) => return Ok(None),
        },
        "source" => match state.storage.get_source(row.entity_id).await {
            Ok(source) => source.title,
            Err(_) => return Ok(None),
        },
        "note" => match state.storage.get_note(row.entity_id).await {
            Ok(note) => {
                if note.text.len() > 96 {
                    format!("{}…", &note.text[..96])
                } else {
                    note.text
                }
            }
            Err(_) => return Ok(None),
        },
        "event" => match state.storage.get_event(row.entity_id).await {
            Ok(event) => display_name_for_event(&event),
            Err(_) => return Ok(None),
        },
        "family" => match state.storage.get_family(row.entity_id).await {
            Ok(family) => format!("Family {}", family.id),
            Err(_) => return Ok(None),
        },
        _ => return Ok(None),
    };

    let match_fields = default_match_fields(&entity_type)
        .into_iter()
        .map(ToString::to_string)
        .collect();

    Ok(Some(SearchResultResponse {
        entity_type,
        entity_id: row.entity_id.0,
        display_name,
        match_fields,
        score: row.score,
        snippet: row.snippet,
    }))
}

async fn passes_post_filters(
    state: &AppState,
    entity_id: EntityId,
    entity_type: &str,
    place_filter: Option<&str>,
    date_from_year: Option<i32>,
    date_to_year: Option<i32>,
) -> Result<bool, ApiError> {
    if let Some(place_query) = place_filter {
        if !matches_place_filter(state, entity_id, entity_type, place_query).await? {
            return Ok(false);
        }
    }

    if date_from_year.is_some() || date_to_year.is_some() {
        let in_range =
            matches_date_filter(state, entity_id, entity_type, date_from_year, date_to_year)
                .await?;
        if !in_range {
            return Ok(false);
        }
    }

    Ok(true)
}

async fn matches_place_filter(
    state: &AppState,
    entity_id: EntityId,
    entity_type: &str,
    place_query: &str,
) -> Result<bool, ApiError> {
    match entity_type {
        "person" => {
            let events = state.storage.list_events_for_person(entity_id).await?;
            for event in events {
                if let Some(place_id) = event.place_ref {
                    if place_matches(state, place_id, place_query).await? {
                        return Ok(true);
                    }
                }
            }
            Ok(false)
        }
        "event" => {
            let event = state.storage.get_event(entity_id).await?;
            if let Some(place_id) = event.place_ref {
                place_matches(state, place_id, place_query).await
            } else {
                Ok(false)
            }
        }
        "place" => place_matches(state, entity_id, place_query).await,
        _ => Ok(true),
    }
}

async fn place_matches(
    state: &AppState,
    place_id: EntityId,
    place_query: &str,
) -> Result<bool, ApiError> {
    let place = state.storage.get_place(place_id).await?;
    Ok(place
        .names
        .iter()
        .any(|name| name.name.to_ascii_lowercase().contains(place_query)))
}

async fn matches_date_filter(
    state: &AppState,
    entity_id: EntityId,
    entity_type: &str,
    from_year: Option<i32>,
    to_year: Option<i32>,
) -> Result<bool, ApiError> {
    match entity_type {
        "person" => {
            let mut years = Vec::new();
            let mut priority_years = Vec::new();
            let events = state.storage.list_events_for_person(entity_id).await?;

            for event in events {
                if let Some(year) = event_year(&event) {
                    years.push(year);
                    if event.event_type == EventType::Birth || event.event_type == EventType::Death
                    {
                        priority_years.push(year);
                    }
                }
            }

            let candidates = if priority_years.is_empty() {
                years
            } else {
                priority_years
            };

            Ok(candidates
                .into_iter()
                .any(|year| is_year_in_range(year, from_year, to_year)))
        }
        "event" => {
            let event = state.storage.get_event(entity_id).await?;
            Ok(event_year(&event)
                .map(|year| is_year_in_range(year, from_year, to_year))
                .unwrap_or(false))
        }
        _ => Ok(false),
    }
}

fn is_year_in_range(year: i32, from_year: Option<i32>, to_year: Option<i32>) -> bool {
    if let Some(from) = from_year {
        if year < from {
            return false;
        }
    }
    if let Some(to) = to_year {
        if year > to {
            return false;
        }
    }
    true
}

fn event_year(event: &Event) -> Option<i32> {
    match event.date.as_ref() {
        Some(DateValue::Exact { date, .. })
        | Some(DateValue::Before { date, .. })
        | Some(DateValue::After { date, .. })
        | Some(DateValue::About { date, .. })
        | Some(DateValue::Tolerance { date, .. }) => Some(date.year),
        Some(DateValue::Range { from, .. }) => Some(from.year),
        Some(DateValue::Quarter { year, .. }) => Some(*year),
        Some(DateValue::Textual { .. }) | None => None,
    }
}

fn parse_year(raw: &str) -> Result<i32, ApiError> {
    let token = raw.trim();
    if token.is_empty() {
        return Err(ApiError::BadRequest(
            "date filters must not be empty when provided".to_string(),
        ));
    }

    let year_part = token.split('-').next().ok_or_else(|| {
        ApiError::BadRequest(format!(
            "invalid date filter: {raw} (expected YYYY or YYYY-MM-DD)"
        ))
    })?;

    year_part.parse::<i32>().map_err(|_| {
        ApiError::BadRequest(format!(
            "invalid date filter: {raw} (expected YYYY or YYYY-MM-DD)"
        ))
    })
}

fn default_match_fields(entity_type: &str) -> Vec<&'static str> {
    match entity_type {
        "person" => vec!["given_name", "surname"],
        "place" => vec!["place"],
        "source" => vec!["title", "author"],
        "note" => vec!["text"],
        "event" => vec!["description", "event_type"],
        "family" => vec!["partner", "child"],
        _ => vec!["content"],
    }
}

fn display_name_for_event(event: &Event) -> String {
    let event_name = match &event.event_type {
        EventType::Birth => "Birth".to_string(),
        EventType::Death => "Death".to_string(),
        EventType::Marriage => "Marriage".to_string(),
        EventType::Census => "Census".to_string(),
        EventType::Baptism => "Baptism".to_string(),
        EventType::Burial => "Burial".to_string(),
        EventType::Migration => "Migration".to_string(),
        EventType::Occupation => "Occupation".to_string(),
        EventType::Residence => "Residence".to_string(),
        EventType::Immigration => "Immigration".to_string(),
        EventType::Emigration => "Emigration".to_string(),
        EventType::Naturalization => "Naturalization".to_string(),
        EventType::Probate => "Probate".to_string(),
        EventType::Will => "Will".to_string(),
        EventType::Graduation => "Graduation".to_string(),
        EventType::Retirement => "Retirement".to_string(),
        EventType::Custom(value) => value.clone(),
    };

    if let Some(description) = &event.description {
        format!("{event_name}: {description}")
    } else {
        event_name
    }
}

fn person_display_name(person: &Person) -> String {
    let primary = person.primary_name();
    let surname = primary
        .surnames
        .iter()
        .map(|item| item.value.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    if surname.is_empty() {
        primary.given_names
    } else if primary.given_names.is_empty() {
        surname
    } else {
        format!("{} {}", primary.given_names, surname)
    }
}

fn build_fts_query(query: &str) -> Result<String, ApiError> {
    let tokens = tokenize_search(query);
    if tokens.is_empty() {
        return Err(ApiError::BadRequest(
            "query parameter q must contain at least one alphanumeric token".to_string(),
        ));
    }

    Ok(tokens
        .into_iter()
        .map(|token| format!("\"{token}\""))
        .collect::<Vec<_>>()
        .join(" AND "))
}

fn build_phonetic_query(query: &str) -> String {
    let mut terms = BTreeSet::new();
    for token in tokenize_search(query) {
        if let Some(code) = soundex(&token) {
            terms.insert(format!("\"sx{}\"", code.to_ascii_lowercase()));
        }
        if let Some(code) = simple_metaphone(&token) {
            terms.insert(format!("\"mp{}\"", code.to_ascii_lowercase()));
        }
    }

    terms.into_iter().collect::<Vec<_>>().join(" OR ")
}

fn tokenize_search(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

async fn fallback_person_matches(
    state: &AppState,
    query: &str,
    strategy: SearchStrategy,
) -> Result<Vec<RawMatch>, ApiError> {
    let persons = state
        .storage
        .list_persons(Pagination {
            limit: 10_000,
            offset: 0,
        })
        .await?;

    let query_tokens = tokenize_search(query);
    let query_lower = query.to_ascii_lowercase();
    let query_soundex = query_tokens
        .iter()
        .filter_map(|token| soundex(token))
        .collect::<BTreeSet<_>>();
    let query_metaphone = query_tokens
        .iter()
        .filter_map(|token| simple_metaphone(token))
        .collect::<BTreeSet<_>>();

    let mut out = Vec::new();
    for person in persons {
        let display_name = person_display_name(&person);
        let display_lower = display_name.to_ascii_lowercase();
        let person_tokens = tokenize_search(&display_name);
        let person_soundex = person_tokens
            .iter()
            .filter_map(|token| soundex(token))
            .collect::<BTreeSet<_>>();
        let person_metaphone = person_tokens
            .iter()
            .filter_map(|token| simple_metaphone(token))
            .collect::<BTreeSet<_>>();

        let exact_match = display_lower.contains(&query_lower);
        let fts_match = query_tokens
            .iter()
            .all(|token| display_lower.contains(token));
        let phonetic_match = !query_soundex.is_empty()
            && (!query_soundex.is_disjoint(&person_soundex)
                || !query_metaphone.is_disjoint(&person_metaphone));

        let include = match strategy {
            SearchStrategy::Exact => exact_match,
            SearchStrategy::Fts => fts_match,
            SearchStrategy::Phonetic => phonetic_match,
            SearchStrategy::Combined => fts_match || phonetic_match,
        };

        if include {
            let score = if exact_match {
                2.0
            } else if fts_match {
                1.5
            } else {
                1.0
            };

            out.push(RawMatch {
                entity_id: person.id,
                entity_type: "person".to_string(),
                score,
                snippet: Some(display_name),
            });
        }
    }

    Ok(out)
}

fn soundex(token: &str) -> Option<String> {
    let mut chars = token.chars().filter(|c| c.is_ascii_alphabetic());
    let first = chars.next()?.to_ascii_uppercase();

    let mut code = String::with_capacity(4);
    code.push(first);

    let mut previous = soundex_digit(first);
    for ch in chars {
        let digit = soundex_digit(ch.to_ascii_uppercase());
        if digit == '0' {
            previous = digit;
            continue;
        }

        if digit != previous {
            code.push(digit);
        }

        previous = digit;

        if code.len() == 4 {
            break;
        }
    }

    while code.len() < 4 {
        code.push('0');
    }

    Some(code)
}

fn soundex_digit(ch: char) -> char {
    match ch {
        'B' | 'F' | 'P' | 'V' => '1',
        'C' | 'G' | 'J' | 'K' | 'Q' | 'S' | 'X' | 'Z' => '2',
        'D' | 'T' => '3',
        'L' => '4',
        'M' | 'N' => '5',
        'R' => '6',
        _ => '0',
    }
}

fn simple_metaphone(token: &str) -> Option<String> {
    let mut chars = token
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase())
        .peekable();

    let mut out = String::new();
    while let Some(ch) = chars.next() {
        let mapped = match ch {
            'A' | 'E' | 'I' | 'O' | 'U' => {
                if out.is_empty() {
                    Some(ch)
                } else {
                    None
                }
            }
            'B' => Some('B'),
            'C' => {
                if chars.peek() == Some(&'H') {
                    chars.next();
                    Some('X')
                } else {
                    Some('K')
                }
            }
            'D' => Some('T'),
            'F' => Some('F'),
            'G' => {
                if chars.peek() == Some(&'H') {
                    chars.next();
                    Some('F')
                } else {
                    Some('K')
                }
            }
            'H' | 'Y' => None,
            'J' => Some('J'),
            'K' | 'Q' => Some('K'),
            'L' => Some('L'),
            'M' | 'N' => Some('N'),
            'P' => {
                if chars.peek() == Some(&'H') {
                    chars.next();
                    Some('F')
                } else {
                    Some('P')
                }
            }
            'R' => Some('R'),
            'S' | 'X' | 'Z' => Some('S'),
            'T' => Some('T'),
            'V' | 'W' => Some('F'),
            _ => None,
        };

        if let Some(code) = mapped {
            let ends_with_same = out.ends_with(code);
            if !ends_with_same {
                out.push(code);
            }
        }
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}
