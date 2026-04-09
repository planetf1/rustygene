use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use axum::body::Body;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use rustygene_core::assertion::AssertionStatus;
use rustygene_core::evidence::{Media, Note, Repository, Source};
use rustygene_core::family::Family;
use rustygene_core::person::Person;
use rustygene_core::types::{ActorRef, EntityId};
use rustygene_gedcom::{
    build_gedcom_tree,
    diff::generate_person_import_diff,
    family_to_fam_node, gramps, import_gedcom_to_sqlite, map_indi_nodes_to_events,
    map_indi_nodes_to_persons,
    matching::{match_persons, MatchConfidence},
    media_to_obje_node, note_to_note_node, person_to_indi_node_with_policy, render_gedcom_file,
    repository_to_repo_node, source_to_sour_node, tokenize_gedcom, ExportPrivacyPolicy,
    GedcomImportError,
};
use rustygene_storage::sqlite_impl::SqliteBackend;
use rustygene_storage::{
    EntityType, JsonAssertion, JsonExportMode, JsonImportMode, Pagination, StorageError,
    StorageErrorCode,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::fs as tokio_fs;
use tokio_util::io::ReaderStream;
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::errors::ApiError;
use crate::{AppState, DomainEvent};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportWarningDetail {
    pub code: String,
    pub title: String,
    pub counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportJobState {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportJobStatus {
    pub job_id: Uuid,
    pub status: ImportJobState,
    pub progress_pct: u8,
    pub entities_imported: Option<usize>,
    pub entities_imported_by_type: Option<BTreeMap<String, usize>>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub warning_details: Vec<ImportWarningDetail>,
    pub log_messages: Vec<String>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct ExportQuery {
    format: ExportFormat,
    #[serde(default)]
    redact_living: bool,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ExportFormat {
    Gedcom,
    Json,
    Bundle,
}

#[derive(Debug, Clone, Copy)]
enum ImportFormat {
    Gedcom,
    GrampsXml,
    Json,
}

#[derive(Debug, Serialize)]
struct ImportAcceptedResponse {
    job_id: Uuid,
    status_url: String,
}

#[derive(Debug, Serialize)]
struct MergeDiffFieldPreview {
    entity_id: EntityId,
    field: String,
    old_value: Value,
    new_value: Value,
    source: String,
    confidence: f64,
}

#[derive(Debug, Serialize)]
struct MergeNewEntityPreview {
    entity_id: EntityId,
    label: String,
    xref: Option<String>,
    fields: Vec<MergeSelection>,
}

#[derive(Debug, Serialize)]
struct MergeDiffResponse {
    changed_fields: Vec<MergeDiffFieldPreview>,
    new_entities: Vec<MergeNewEntityPreview>,
    unchanged_entities: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct MergeSelection {
    entity_type: String,
    entity_id: EntityId,
    field: String,
    new_value: Value,
    source: Option<String>,
    confidence: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ImportMergeRequest {
    selected_changes: Vec<MergeSelection>,
    #[serde(default)]
    submitted_by: Option<String>,
}

#[derive(Debug, Serialize)]
struct ImportMergeResponse {
    proposals_created: usize,
    proposal_ids: Vec<EntityId>,
}

#[derive(Debug, Serialize)]
struct BundleManifest {
    version: String,
    exported_at: String,
    entity_counts: BTreeMap<String, usize>,
    files: Vec<String>,
}

#[derive(Debug)]
struct ImportExecutionSummary {
    entities_imported: usize,
    entities_imported_by_type: BTreeMap<String, usize>,
    warnings: Vec<String>,
    warning_details: Vec<ImportWarningDetail>,
    log_messages: Vec<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/import", post(start_import))
        .route("/import/diff", post(import_diff))
        .route("/import/merge", post(import_merge))
        .route("/import/:job_id", get(get_import_job_status))
        .route("/export", get(export_data))
}

pub fn legacy_router() -> Router<AppState> {
    router()
}

async fn start_import(
    State(state): State<AppState>,
    multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    let (format, input, file_name) = extract_import_upload(multipart).await?;

    let job_id = Uuid::new_v4();
    let initial_status = ImportJobStatus {
        job_id,
        status: ImportJobState::Queued,
        progress_pct: 0,
        entities_imported: None,
        entities_imported_by_type: None,
        errors: Vec::new(),
        warnings: Vec::new(),
        warning_details: Vec::new(),
        log_messages: vec!["Import queued.".to_string()],
        completed_at: None,
    };

    {
        let mut jobs = state.import_jobs.write().await;
        jobs.insert(job_id, initial_status);
    }

    let jobs = state.import_jobs.clone();
    let sqlite_backend = state.sqlite_backend.clone();
    let event_bus = state.event_bus.clone();
    let submitted_file_name = file_name.clone();

    tokio::spawn(async move {
        update_job(&jobs, job_id, |status| {
            status.status = ImportJobState::Running;
            status.progress_pct = 15;
            if let Some(name) = &submitted_file_name {
                status.log_messages.push(format!("Selected file: {name}"));
            }
            status
                .log_messages
                .push(format!("Detected format: {}", import_format_label(format)));
            status.log_messages.push(match format {
                ImportFormat::Gedcom => "Parsing GEDCOM file...".to_string(),
                ImportFormat::Json => "Parsing JSON import payload...".to_string(),
                ImportFormat::GrampsXml => "Preparing Gramps XML import...".to_string(),
            });
        })
        .await;

        let Some(backend) = sqlite_backend else {
            update_job(&jobs, job_id, |status| {
                status.status = ImportJobState::Failed;
                status.progress_pct = 100;
                status
                    .errors
                    .push("sqlite backend not available for import/export".to_string());
                status.log_messages.push(
                    "Import failed: sqlite backend not available for import/export.".to_string(),
                );
                status.completed_at = Some(Utc::now());
            })
            .await;
            return;
        };

        let result = tokio::task::spawn_blocking(move || match format {
            ImportFormat::Gedcom => run_gedcom_import(&backend, job_id, &input),
            ImportFormat::Json => run_json_import(&backend, &input),
            ImportFormat::GrampsXml => run_gramps_import(&backend, job_id, &input),
        })
        .await;

        match result {
            Ok(Ok(summary)) => {
                update_job(&jobs, job_id, |status| {
                    status.status = ImportJobState::Completed;
                    status.progress_pct = 100;
                    status.entities_imported = Some(summary.entities_imported);
                    status.entities_imported_by_type =
                        Some(summary.entities_imported_by_type.clone());
                    status.warnings = summary.warnings.clone();
                    status.warning_details = summary.warning_details.clone();
                    status.log_messages.extend(summary.log_messages.clone());
                    status.log_messages.push(format!(
                        "Import completed: {} total entities.",
                        summary.entities_imported
                    ));
                    status.completed_at = Some(Utc::now());
                })
                .await;

                let _ = event_bus.send(DomainEvent::ImportCompleted {
                    job_id,
                    entities_imported: summary.entities_imported_by_type,
                    timestamp: Utc::now().to_rfc3339(),
                });
            }
            Ok(Err(err)) => {
                update_job(&jobs, job_id, |status| {
                    status.status = ImportJobState::Failed;
                    status.progress_pct = 100;
                    status.errors.push(err.message());
                    status
                        .log_messages
                        .push(format!("Import failed: {}", err.message()));
                    status.completed_at = Some(Utc::now());
                })
                .await;
            }
            Err(join_err) => {
                update_job(&jobs, job_id, |status| {
                    status.status = ImportJobState::Failed;
                    status.progress_pct = 100;
                    status
                        .errors
                        .push(format!("import task join error: {join_err}"));
                    status
                        .log_messages
                        .push(format!("Import failed: import task join error: {join_err}"));
                    status.completed_at = Some(Utc::now());
                })
                .await;
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(ImportAcceptedResponse {
            job_id,
            status_url: format!("/api/v1/import/{job_id}"),
        }),
    ))
}

async fn get_import_job_status(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<ImportJobStatus>, ApiError> {
    let parsed_job_id =
        Uuid::parse_str(&job_id).map_err(|_| ApiError::BadRequest("invalid job_id".to_string()))?;

    let jobs = state.import_jobs.read().await;
    let status = jobs
        .get(&parsed_job_id)
        .cloned()
        .ok_or_else(|| ApiError::NotFound(format!("job not found: {parsed_job_id}")))?;

    Ok(Json(status))
}

async fn import_diff(
    State(state): State<AppState>,
    multipart: Multipart,
) -> Result<Json<MergeDiffResponse>, ApiError> {
    let (format, input, file_name) = extract_import_upload(multipart).await?;
    if !matches!(format, ImportFormat::Gedcom) {
        return Err(ApiError::BadRequest(
            "import diff currently supports only GEDCOM uploads".to_string(),
        ));
    }

    let text = std::str::from_utf8(&input)
        .map_err(|err| ApiError::BadRequest(format!("GEDCOM file must be UTF-8 text: {err}")))?;
    let lines = tokenize_gedcom(text)
        .map_err(|err| ApiError::BadRequest(format!("invalid GEDCOM payload: {err}")))?;
    let nodes = build_gedcom_tree(&lines)
        .map_err(|err| ApiError::BadRequest(format!("invalid GEDCOM structure: {err}")))?;

    let gedcom_persons = map_indi_nodes_to_persons(&nodes);
    let gedcom_events = map_indi_nodes_to_events(&nodes);

    let existing_persons = state
        .storage
        .list_persons(Pagination {
            limit: 10_000,
            offset: 0,
        })
        .await?;
    let existing_events = state
        .storage
        .list_events(Pagination {
            limit: 20_000,
            offset: 0,
        })
        .await?;

    let prior_xref_map = existing_persons
        .iter()
        .filter_map(|person| person.original_xref.clone().map(|xref| (xref, person.id)))
        .collect::<std::collections::HashMap<_, _>>();

    let match_result = match_persons(
        &gedcom_persons,
        &gedcom_events,
        &existing_persons,
        &existing_events,
        &prior_xref_map,
    );
    let source_name = file_name
        .as_deref()
        .unwrap_or("uploaded-merge-preview.ged")
        .to_string();
    let diff = generate_person_import_diff(
        &match_result,
        &gedcom_persons,
        &gedcom_events,
        &existing_persons,
        &existing_events,
        &source_name,
    );

    let confidence_by_entity = match_result
        .matched
        .iter()
        .map(|entry| {
            (
                entry.entity_id,
                match entry.confidence {
                    MatchConfidence::Exact => 1.0,
                    MatchConfidence::High => 0.9,
                    MatchConfidence::Medium => 0.7,
                },
            )
        })
        .collect::<std::collections::HashMap<_, _>>();

    let changed_fields = diff
        .updated_fields
        .into_iter()
        .map(|field| MergeDiffFieldPreview {
            entity_id: field.entity_id,
            field: field.field,
            old_value: field.old_value,
            new_value: field.new_value,
            source: field.source,
            confidence: confidence_by_entity
                .get(&field.entity_id)
                .copied()
                .unwrap_or(0.8),
        })
        .collect::<Vec<_>>();

    let new_entities = diff
        .new_entities
        .into_iter()
        .filter_map(|entry| {
            let rustygene_gedcom::matching::GedcomRef::Person { xref, label } = entry else {
                return None;
            };

            let person = if let Some(ref xref_value) = xref {
                gedcom_persons
                    .iter()
                    .find(|p| p.original_xref.as_deref() == Some(xref_value.as_str()))
            } else {
                gedcom_persons.iter().find(|p| {
                    let primary = p.primary_name();
                    let surname = primary
                        .surnames
                        .first()
                        .map(|s| s.value.as_str())
                        .unwrap_or_default();
                    format!("{} {}", primary.given_names.trim(), surname.trim())
                        .trim()
                        .eq_ignore_ascii_case(label.as_str())
                })
            }?;

            let fields = person_fields_for_merge(person, &gedcom_events, &source_name)
                .into_iter()
                .map(|(field, new_value)| MergeSelection {
                    entity_type: "person".to_string(),
                    entity_id: person.id,
                    field,
                    new_value,
                    source: Some(source_name.clone()),
                    confidence: Some(0.7),
                })
                .collect::<Vec<_>>();

            Some(MergeNewEntityPreview {
                entity_id: person.id,
                label,
                xref,
                fields,
            })
        })
        .collect::<Vec<_>>();

    Ok(Json(MergeDiffResponse {
        changed_fields,
        new_entities,
        unchanged_entities: diff.unchanged,
    }))
}

async fn import_merge(
    State(state): State<AppState>,
    Json(request): Json<ImportMergeRequest>,
) -> Result<(StatusCode, Json<ImportMergeResponse>), ApiError> {
    if request.selected_changes.is_empty() {
        return Err(ApiError::BadRequest(
            "selected_changes must not be empty".to_string(),
        ));
    }

    let submitted_by = request.submitted_by.as_deref().unwrap_or("import-merge");
    let mut proposal_ids = Vec::with_capacity(request.selected_changes.len());

    for selection in request.selected_changes {
        if selection.field.trim().is_empty() {
            return Err(ApiError::BadRequest(
                "selection field must not be empty".to_string(),
            ));
        }

        let entity_type = parse_merge_entity_type(&selection.entity_type)?;
        let source = selection
            .source
            .clone()
            .unwrap_or_else(|| "gedcom-merge".to_string());

        let assertion = JsonAssertion {
            id: EntityId::new(),
            value: selection.new_value,
            confidence: selection.confidence.unwrap_or(0.8),
            status: AssertionStatus::Proposed,
            evidence_type: rustygene_core::assertion::EvidenceType::Direct,
            source_citations: Vec::new(),
            proposed_by: ActorRef::Import(source),
            created_at: Utc::now(),
            reviewed_at: None,
            reviewed_by: None,
        };

        let proposal_id = match state
            .storage
            .submit_staging_proposal(
                selection.entity_id,
                entity_type,
                &selection.field,
                &assertion,
                submitted_by,
            )
            .await
        {
            Ok(id) => id,
            Err(err) if err.code == StorageErrorCode::NotFound => {
                return Err(ApiError::BadRequest(format!(
                    "selected change references unknown entity {}; promote existing entities first",
                    selection.entity_id
                )));
            }
            Err(err) => return Err(ApiError::from(err)),
        };
        proposal_ids.push(proposal_id);
    }

    Ok((
        StatusCode::CREATED,
        Json(ImportMergeResponse {
            proposals_created: proposal_ids.len(),
            proposal_ids,
        }),
    ))
}

async fn extract_import_upload(
    mut multipart: Multipart,
) -> Result<(ImportFormat, Vec<u8>, Option<String>), ApiError> {
    let mut import_format: Option<ImportFormat> = None;
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut file_name: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| ApiError::BadRequest(format!("failed to read multipart field: {err}")))?
    {
        let Some(name) = field.name().map(ToString::to_string) else {
            continue;
        };

        match name.as_str() {
            "format" => {
                let raw = field.text().await.map_err(|err| {
                    ApiError::BadRequest(format!("failed to read format field: {err}"))
                })?;
                import_format = Some(parse_import_format(&raw)?);
            }
            "file" => {
                file_name = field.file_name().map(ToString::to_string);
                let bytes = field.bytes().await.map_err(|err| {
                    ApiError::BadRequest(format!("failed to read file field: {err}"))
                })?;
                file_bytes = Some(bytes.to_vec());
            }
            _ => {}
        }
    }

    let format =
        import_format.ok_or_else(|| ApiError::BadRequest("missing format field".to_string()))?;
    let input = file_bytes.ok_or_else(|| ApiError::BadRequest("missing file field".to_string()))?;
    Ok((format, input, file_name))
}

async fn export_data(
    State(state): State<AppState>,
    Query(query): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    let Some(backend) = state.sqlite_backend.clone() else {
        return Err(ApiError::InternalError(
            "sqlite backend not available for import/export".to_string(),
        ));
    };

    let payload = tokio::task::spawn_blocking(move || match query.format {
        ExportFormat::Gedcom => {
            let bytes = export_gedcom_bytes(&backend, query.redact_living)?;
            let path = write_temp_payload("ged", &bytes)?;
            Ok::<(PathBuf, &'static str, &'static str), ApiError>((
                path,
                "application/octet-stream",
                "rustygene-export.ged",
            ))
        }
        ExportFormat::Json => {
            let path =
                std::env::temp_dir().join(format!("rustygene-export-{}.json", Uuid::new_v4()));
            backend
                .export_json_dump(JsonExportMode::SingleFile {
                    output_file: path.clone(),
                })
                .map_err(ApiError::from)?;

            Ok((path, "application/json", "rustygene-export.json"))
        }
        ExportFormat::Bundle => {
            let path = export_bundle_zip(&backend)?;
            Ok((path, "application/zip", "rustygene-export-bundle.zip"))
        }
    })
    .await
    .map_err(|err| ApiError::InternalError(format!("export task join error: {err}")))??;

    stream_file_response(payload.0, payload.1, payload.2).await
}

fn run_gedcom_import(
    backend: &SqliteBackend,
    job_id: Uuid,
    input: &[u8],
) -> Result<ImportExecutionSummary, ApiError> {
    let text = std::str::from_utf8(input)
        .map_err(|err| ApiError::BadRequest(format!("GEDCOM file must be UTF-8 text: {err}")))?;

    let report = backend.with_connection(|conn| {
        import_gedcom_to_sqlite(conn, &job_id.to_string(), text).map_err(map_gedcom_import_error)
    })?;
    backend.rebuild_search_index().map_err(ApiError::from)?;

    let entities_imported = report
        .entities_created_by_type
        .values()
        .copied()
        .sum::<usize>();
    let entities_imported_by_type = report
        .entities_created_by_type
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect::<BTreeMap<_, _>>();

    let mut warnings = Vec::new();
    let mut warning_details = Vec::new();
    if !report.deferred_standard_tags.is_empty() {
        warnings.push(format!(
            "Unhandled standard GEDCOM tags: {} distinct tag(s)",
            report.deferred_standard_tags.len()
        ));
        warning_details.push(ImportWarningDetail {
            code: "deferred_standard_tags".to_string(),
            title: "Unhandled standard GEDCOM tags".to_string(),
            counts: report.deferred_standard_tags.clone(),
        });
    }
    if !report.unhandled_tags.is_empty() {
        warnings.push(format!(
            "Unhandled custom GEDCOM tags: {} distinct tag(s)",
            report.unhandled_tags.len()
        ));
        warning_details.push(ImportWarningDetail {
            code: "unhandled_custom_tags".to_string(),
            title: "Unhandled custom GEDCOM tags".to_string(),
            counts: report.unhandled_tags.clone(),
        });
    }

    let mut log_messages = vec!["GEDCOM parsing complete.".to_string()];
    log_messages.push("Rebuilt search index from imported assertions.".to_string());
    log_messages.push(format!(
        "Imported entities by type: {}",
        format_counts_inline(&entities_imported_by_type)
    ));
    if !warning_details.is_empty() {
        log_messages.push(format!(
            "Warnings recorded: {}",
            warning_details
                .iter()
                .map(|detail| format!("{} ({})", detail.title, detail.counts.len()))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    Ok(ImportExecutionSummary {
        entities_imported,
        entities_imported_by_type,
        warnings,
        warning_details,
        log_messages,
    })
}

fn run_gramps_import(
    backend: &SqliteBackend,
    job_id: Uuid,
    input: &[u8],
) -> Result<ImportExecutionSummary, ApiError> {
    let text = std::str::from_utf8(input).map_err(|err| {
        ApiError::BadRequest(format!("Gramps XML file must be UTF-8 text: {err}"))
    })?;

    let report = gramps::import_gramps_xml_to_sqlite(backend, &job_id.to_string(), text)
        .map_err(|err| ApiError::InternalError(format!("Gramps import failed: {err}")))?;
    backend.rebuild_search_index().map_err(ApiError::from)?;

    let entities_imported = report
        .entities_created_by_type
        .values()
        .copied()
        .sum::<usize>();
    let entities_imported_by_type = report
        .entities_created_by_type
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect::<BTreeMap<_, _>>();

    let mut log_messages = vec!["Gramps XML parsing complete.".to_string()];
    log_messages.push("Rebuilt search index from imported assertions.".to_string());
    log_messages.push(format!(
        "Imported entities by type: {}",
        format_counts_inline(&entities_imported_by_type)
    ));
    log_messages.push(format!("Assertions created: {}", report.assertions_created));

    Ok(ImportExecutionSummary {
        entities_imported,
        entities_imported_by_type,
        warnings: Vec::new(),
        warning_details: Vec::new(),
        log_messages,
    })
}

fn run_json_import(
    backend: &SqliteBackend,
    input: &[u8],
) -> Result<ImportExecutionSummary, ApiError> {
    let file_path = write_temp_payload("json", input)?;
    let report = backend
        .import_json_dump(JsonImportMode::SingleFile {
            input_file: file_path,
        })
        .map_err(ApiError::from)?;

    let entities_imported = report
        .entities_imported_by_type
        .values()
        .copied()
        .sum::<usize>();
    let entities_imported_by_type = report
        .entities_imported_by_type
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect::<BTreeMap<_, _>>();

    Ok(ImportExecutionSummary {
        entities_imported,
        entities_imported_by_type: entities_imported_by_type.clone(),
        warnings: Vec::new(),
        warning_details: Vec::new(),
        log_messages: vec![
            "JSON import parsing complete.".to_string(),
            format!(
                "Imported entities by type: {}",
                format_counts_inline(&entities_imported_by_type)
            ),
        ],
    })
}

fn export_gedcom_bytes(backend: &SqliteBackend, redact_living: bool) -> Result<Vec<u8>, ApiError> {
    let privacy_policy = if redact_living {
        ExportPrivacyPolicy::RedactLiving
    } else {
        ExportPrivacyPolicy::None
    };

    let rendered = backend.with_connection(|conn| {
        let persons: Vec<Person> = load_snapshot_entities(conn, "persons")?;
        let families: Vec<Family> = load_family_entities(conn)?;
        let sources: Vec<Source> = load_snapshot_entities(conn, "sources")?;
        let repositories: Vec<Repository> = load_snapshot_entities(conn, "repositories")?;
        let notes: Vec<Note> = load_snapshot_entities(conn, "notes")?;
        let media: Vec<Media> = load_snapshot_entities(conn, "media")?;
        let events: Vec<rustygene_core::event::Event> = load_snapshot_entities(conn, "events")?;
        let places: Vec<rustygene_core::place::Place> = load_snapshot_entities(conn, "places")?;

        let mut nodes = Vec::new();
        for (idx, person) in persons.iter().enumerate() {
            let xref = preserved_or_generated_xref(person.original_xref.as_deref(), 'I', idx);
            if let Some(node) =
                person_to_indi_node_with_policy(person, &events, &places, &xref, privacy_policy)
            {
                nodes.push(node);
            }
        }
        for (idx, family) in families.iter().enumerate() {
            let xref = preserved_or_generated_xref(family.original_xref.as_deref(), 'F', idx);
            nodes.push(family_to_fam_node(
                family, &persons, &events, &places, &xref,
            ));
        }
        for (idx, source) in sources.iter().enumerate() {
            let xref = preserved_or_generated_xref(source.original_xref.as_deref(), 'S', idx);
            nodes.push(source_to_sour_node(source, &xref));
        }
        for (idx, repository) in repositories.iter().enumerate() {
            let xref = preserved_or_generated_xref(repository.original_xref.as_deref(), 'R', idx);
            nodes.push(repository_to_repo_node(repository, &xref));
        }
        for (idx, note) in notes.iter().enumerate() {
            let xref = preserved_or_generated_xref(note.original_xref.as_deref(), 'N', idx);
            nodes.push(note_to_note_node(note, &xref));
        }
        for (idx, item) in media.iter().enumerate() {
            let xref = preserved_or_generated_xref(item.original_xref.as_deref(), 'O', idx);
            nodes.push(media_to_obje_node(item, &xref));
        }

        Ok(render_gedcom_file(&nodes))
    })?;

    Ok(rendered.into_bytes())
}

fn export_bundle_zip(backend: &SqliteBackend) -> Result<PathBuf, ApiError> {
    let temp_dir = std::env::temp_dir().join(format!("rustygene-export-bundle-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).map_err(|err| {
        ApiError::InternalError(format!("failed to create bundle temp directory: {err}"))
    })?;

    let db_json_path = temp_dir.join("database.json");
    let export_result = backend
        .export_json_dump(JsonExportMode::SingleFile {
            output_file: db_json_path.clone(),
        })
        .map_err(ApiError::from)?;

    let media_rows: Vec<Media> =
        backend.with_connection(|conn| load_snapshot_entities(conn, "media"))?;

    let zip_path = temp_dir.join("bundle.zip");
    let zip_file = File::create(&zip_path)
        .map_err(|err| ApiError::InternalError(format!("failed to create bundle zip: {err}")))?;
    let mut zip = ZipWriter::new(zip_file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut files = Vec::new();

    let database_bytes = std::fs::read(&db_json_path)
        .map_err(|err| ApiError::InternalError(format!("failed to read database export: {err}")))?;
    zip.start_file("database.json", options).map_err(|err| {
        ApiError::InternalError(format!("failed to add database.json to bundle: {err}"))
    })?;
    zip.write_all(&database_bytes).map_err(|err| {
        ApiError::InternalError(format!("failed to write database.json to bundle: {err}"))
    })?;
    files.push("database.json".to_string());

    for media in media_rows {
        let source_path = PathBuf::from(&media.file_path);
        if !source_path.exists() {
            continue;
        }

        let Some(file_name) = source_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        let entry_name = format!("media/{file_name}");
        let bytes = std::fs::read(&source_path).map_err(|err| {
            ApiError::InternalError(format!(
                "failed to read media file '{}': {err}",
                source_path.display()
            ))
        })?;
        zip.start_file(&entry_name, options).map_err(|err| {
            ApiError::InternalError(format!("failed to add '{entry_name}' to bundle: {err}"))
        })?;
        zip.write_all(&bytes).map_err(|err| {
            ApiError::InternalError(format!("failed to write '{entry_name}' to bundle: {err}"))
        })?;
        files.push(entry_name);
    }

    let manifest = BundleManifest {
        version: "1".to_string(),
        exported_at: Utc::now().to_rfc3339(),
        entity_counts: export_result.manifest.entity_counts,
        files,
    };

    zip.start_file("manifest.json", options).map_err(|err| {
        ApiError::InternalError(format!("failed to add manifest.json to bundle: {err}"))
    })?;
    let manifest_json = serde_json::to_vec_pretty(&manifest).map_err(|err| {
        ApiError::InternalError(format!("failed to serialize bundle manifest: {err}"))
    })?;
    zip.write_all(&manifest_json).map_err(|err| {
        ApiError::InternalError(format!("failed to write manifest.json to bundle: {err}"))
    })?;

    zip.finish()
        .map_err(|err| ApiError::InternalError(format!("failed to finalize bundle zip: {err}")))?;

    Ok(zip_path)
}

async fn stream_file_response(
    path: PathBuf,
    content_type: &'static str,
    file_name: &'static str,
) -> Result<Response, ApiError> {
    let file = tokio_fs::File::open(&path)
        .await
        .map_err(|err| ApiError::InternalError(format!("failed to open export file: {err}")))?;
    let stream = ReaderStream::new(file);

    let mut response = Body::from_stream(stream).into_response();
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    response.headers_mut().insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{file_name}\"")).map_err(|err| {
            ApiError::InternalError(format!("invalid content-disposition: {err}"))
        })?,
    );

    Ok(response)
}

fn parse_import_format(raw: &str) -> Result<ImportFormat, ApiError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "gedcom" => Ok(ImportFormat::Gedcom),
        "gramps_xml" => Ok(ImportFormat::GrampsXml),
        "json" => Ok(ImportFormat::Json),
        other => Err(ApiError::BadRequest(format!(
            "unsupported import format: {other}"
        ))),
    }
}

fn parse_merge_entity_type(raw: &str) -> Result<EntityType, ApiError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "person" => Ok(EntityType::Person),
        "family" => Ok(EntityType::Family),
        "relationship" => Ok(EntityType::Relationship),
        "event" => Ok(EntityType::Event),
        "place" => Ok(EntityType::Place),
        "source" => Ok(EntityType::Source),
        "citation" => Ok(EntityType::Citation),
        "repository" => Ok(EntityType::Repository),
        "media" => Ok(EntityType::Media),
        "note" => Ok(EntityType::Note),
        "lds_ordinance" | "ldsordinance" => Ok(EntityType::LdsOrdinance),
        other => Err(ApiError::BadRequest(format!(
            "invalid merge entity type: {other}"
        ))),
    }
}

fn person_fields_for_merge(
    person: &Person,
    events: &[rustygene_core::event::Event],
    source_name: &str,
) -> Vec<(String, Value)> {
    let primary = person.primary_name();
    let mut out = Vec::new();

    out.push((
        "name.given".to_string(),
        Value::String(primary.given_names.clone()),
    ));
    out.push((
        "name.surname".to_string(),
        Value::String(
            primary
                .surnames
                .first()
                .map(|surname| surname.value.clone())
                .unwrap_or_default(),
        ),
    ));
    out.push((
        "gender".to_string(),
        Value::String(format!("{:?}", person.gender)),
    ));

    if let Some(date) = person_event_date_for_merge(person.id, events, "birth") {
        out.push(("birth.date".to_string(), date));
    }
    if let Some(date) = person_event_date_for_merge(person.id, events, "death") {
        out.push(("death.date".to_string(), date));
    }

    out.into_iter()
        .filter(|(_, value)| !matches!(value, Value::String(v) if v.trim().is_empty()))
        .map(|(field, value)| {
            let normalized = if field == "source" {
                Value::String(source_name.to_string())
            } else {
                value
            };
            (field, normalized)
        })
        .collect()
}

fn person_event_date_for_merge(
    person_id: EntityId,
    events: &[rustygene_core::event::Event],
    kind: &str,
) -> Option<Value> {
    let event_type = match kind {
        "birth" => rustygene_core::event::EventType::Birth,
        "death" => rustygene_core::event::EventType::Death,
        _ => return None,
    };

    events
        .iter()
        .find(|event| {
            event.event_type == event_type
                && event
                    .participants
                    .iter()
                    .any(|participant| participant.person_id == person_id)
        })
        .and_then(|event| event.date.as_ref())
        .map(|date| serde_json::to_value(date).unwrap_or(Value::Null))
}

fn import_format_label(format: ImportFormat) -> &'static str {
    match format {
        ImportFormat::Gedcom => "GEDCOM 5.5.1",
        ImportFormat::GrampsXml => "Gramps XML",
        ImportFormat::Json => "JSON",
    }
}

fn format_counts_inline(counts: &BTreeMap<String, usize>) -> String {
    counts
        .iter()
        .map(|(label, count)| format!("{label}: {count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

async fn update_job<F>(
    jobs: &tokio::sync::RwLock<std::collections::HashMap<Uuid, ImportJobStatus>>,
    job_id: Uuid,
    mut update: F,
) where
    F: FnMut(&mut ImportJobStatus),
{
    let mut write_guard = jobs.write().await;
    if let Some(status) = write_guard.get_mut(&job_id) {
        update(status);
    }
}

fn map_gedcom_import_error(error: GedcomImportError) -> StorageError {
    StorageError {
        code: StorageErrorCode::Backend,
        message: error.to_string(),
    }
}

fn write_temp_payload(extension: &str, bytes: &[u8]) -> Result<PathBuf, ApiError> {
    let path = std::env::temp_dir().join(format!(
        "rustygene-import-export-{}.{}",
        Uuid::new_v4(),
        extension
    ));
    std::fs::write(&path, bytes)
        .map_err(|err| ApiError::InternalError(format!("failed to write temp payload: {err}")))?;
    Ok(path)
}

fn preserved_or_generated_xref(original: Option<&str>, prefix: char, index: usize) -> String {
    if let Some(value) = original {
        if value.starts_with('@') && value.ends_with('@') {
            return value.to_string();
        }
    }

    format!("@{}{}@", prefix, index + 1)
}

fn load_snapshot_entities<T: serde::de::DeserializeOwned>(
    conn: &rusqlite::Connection,
    table: &str,
) -> Result<Vec<T>, StorageError> {
    let mut stmt = conn
        .prepare(&format!("SELECT data FROM {} ORDER BY created_at", table))
        .map_err(|err| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("prepare {} query failed: {err}", table),
        })?;

    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|err| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("query {} failed: {err}", table),
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("collect {} failed: {err}", table),
        })?;

    rows.into_iter()
        .map(|raw| {
            serde_json::from_str::<T>(&raw).map_err(|err| StorageError {
                code: StorageErrorCode::Serialization,
                message: format!("parse {} row failed: {err}", table),
            })
        })
        .collect()
}

fn load_family_entities(conn: &rusqlite::Connection) -> Result<Vec<Family>, StorageError> {
    let mut stmt = conn
        .prepare(
            "SELECT data FROM families WHERE json_extract(data, '$.relationship_type') IS NULL ORDER BY created_at",
        )
        .map_err(|err| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("prepare families query failed: {err}"),
        })?;

    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|err| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("query families failed: {err}"),
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("collect families failed: {err}"),
        })?;

    rows.into_iter()
        .map(|raw| {
            serde_json::from_str::<Family>(&raw).map_err(|err| StorageError {
                code: StorageErrorCode::Serialization,
                message: format!("parse families row failed: {err}"),
            })
        })
        .collect()
}
