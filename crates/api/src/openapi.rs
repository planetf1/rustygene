use axum::extract::State;
use axum::{Json, Router};
use utoipa::OpenApi;

use crate::AppState;

macro_rules! spec_path {
    ($name:ident, get, $path:literal) => {
        #[allow(dead_code)]
        #[utoipa::path(get, path = $path, responses((status = 200, description = "OK")))]
        fn $name() {}
    };
    ($name:ident, post, $path:literal) => {
        #[allow(dead_code)]
        #[utoipa::path(post, path = $path, responses((status = 200, description = "OK"), (status = 201, description = "Created"), (status = 202, description = "Accepted")))]
        fn $name() {}
    };
    ($name:ident, put, $path:literal) => {
        #[allow(dead_code)]
        #[utoipa::path(put, path = $path, responses((status = 200, description = "OK")))]
        fn $name() {}
    };
    ($name:ident, delete, $path:literal) => {
        #[allow(dead_code)]
        #[utoipa::path(delete, path = $path, responses((status = 200, description = "OK"), (status = 204, description = "No Content")))]
        fn $name() {}
    };
}

spec_path!(health_get, get, "/api/v1/health");

spec_path!(persons_list, get, "/api/v1/persons");
spec_path!(persons_create, post, "/api/v1/persons");
spec_path!(persons_get, get, "/api/v1/persons/{id}");
spec_path!(persons_update, put, "/api/v1/persons/{id}");
spec_path!(persons_delete, delete, "/api/v1/persons/{id}");
spec_path!(
    persons_assertions_list,
    get,
    "/api/v1/persons/{id}/assertions"
);
spec_path!(
    persons_assertions_create,
    post,
    "/api/v1/persons/{id}/assertions"
);
spec_path!(
    persons_assertions_update,
    put,
    "/api/v1/persons/{id}/assertions/{assertion_id}"
);
spec_path!(persons_timeline, get, "/api/v1/persons/{id}/timeline");
spec_path!(persons_families, get, "/api/v1/persons/{id}/families");

spec_path!(families_list, get, "/api/v1/families");
spec_path!(families_create, post, "/api/v1/families");
spec_path!(families_get, get, "/api/v1/families/{id}");
spec_path!(families_update, put, "/api/v1/families/{id}");
spec_path!(families_delete, delete, "/api/v1/families/{id}");
spec_path!(families_assertions, get, "/api/v1/families/{id}/assertions");

spec_path!(events_list, get, "/api/v1/events");
spec_path!(events_create, post, "/api/v1/events");
spec_path!(events_get, get, "/api/v1/events/{id}");
spec_path!(events_update, put, "/api/v1/events/{id}");
spec_path!(events_delete, delete, "/api/v1/events/{id}");
spec_path!(
    events_assertions_list,
    get,
    "/api/v1/events/{id}/assertions"
);
spec_path!(
    events_assertions_create,
    post,
    "/api/v1/events/{id}/assertions"
);
spec_path!(
    events_participants_add,
    post,
    "/api/v1/events/{id}/participants"
);
spec_path!(
    events_participants_remove,
    delete,
    "/api/v1/events/{id}/participants/{pid}"
);
spec_path!(events_stream, get, "/api/v1/events/stream");

spec_path!(search_get, get, "/api/v1/search");

spec_path!(graph_ancestors, get, "/api/v1/graph/ancestors/{id}");
spec_path!(graph_descendants, get, "/api/v1/graph/descendants/{id}");
spec_path!(graph_pedigree, get, "/api/v1/graph/pedigree/{id}");
spec_path!(graph_path, get, "/api/v1/graph/path/{id1}/{id2}");
spec_path!(graph_network, get, "/api/v1/graph/network/{id}");

spec_path!(sources_list, get, "/api/v1/sources");
spec_path!(sources_create, post, "/api/v1/sources");
spec_path!(sources_get, get, "/api/v1/sources/{id}");
spec_path!(sources_update, put, "/api/v1/sources/{id}");
spec_path!(sources_delete, delete, "/api/v1/sources/{id}");

spec_path!(citations_list, get, "/api/v1/citations");
spec_path!(citations_create, post, "/api/v1/citations");
spec_path!(citations_get, get, "/api/v1/citations/{id}");
spec_path!(citations_update, put, "/api/v1/citations/{id}");
spec_path!(citations_delete, delete, "/api/v1/citations/{id}");

spec_path!(repositories_list, get, "/api/v1/repositories");
spec_path!(repositories_create, post, "/api/v1/repositories");
spec_path!(repositories_get, get, "/api/v1/repositories/{id}");
spec_path!(repositories_update, put, "/api/v1/repositories/{id}");
spec_path!(repositories_delete, delete, "/api/v1/repositories/{id}");

spec_path!(notes_list, get, "/api/v1/notes");
spec_path!(notes_create, post, "/api/v1/notes");
spec_path!(notes_get, get, "/api/v1/notes/{id}");
spec_path!(notes_update, put, "/api/v1/notes/{id}");
spec_path!(notes_delete, delete, "/api/v1/notes/{id}");

spec_path!(research_log_list, get, "/api/v1/research-log");
spec_path!(research_log_create, post, "/api/v1/research-log");
spec_path!(research_log_get, get, "/api/v1/research-log/{id}");
spec_path!(research_log_update, put, "/api/v1/research-log/{id}");
spec_path!(research_log_delete, delete, "/api/v1/research-log/{id}");

spec_path!(media_list, get, "/api/v1/media");
spec_path!(media_upload, post, "/api/v1/media");
spec_path!(media_get, get, "/api/v1/media/{id}");
spec_path!(media_delete, delete, "/api/v1/media/{id}");
spec_path!(media_links, get, "/api/v1/media/{id}/links");
spec_path!(media_update_text, put, "/api/v1/media/{id}/text");
spec_path!(media_tags_add, post, "/api/v1/media/{id}/tags");
spec_path!(media_tags_remove, delete, "/api/v1/media/{id}/tags/{tag}");
spec_path!(media_file, get, "/api/v1/media/{id}/file");
spec_path!(media_thumbnail, get, "/api/v1/media/{id}/thumbnail");
spec_path!(media_extract_get, get, "/api/v1/media/{id}/extract");
spec_path!(media_extract_post, post, "/api/v1/media/{id}/extract");
spec_path!(media_albums_list, get, "/api/v1/media/albums");
spec_path!(media_albums_create, post, "/api/v1/media/albums");
spec_path!(
    media_album_items_add,
    post,
    "/api/v1/media/albums/{album_id}/items"
);

spec_path!(entity_media_list, get, "/api/v1/entities/{entity_id}/media");
spec_path!(
    entity_media_link,
    post,
    "/api/v1/entities/{entity_id}/media/{media_id}"
);
spec_path!(
    entity_media_unlink,
    delete,
    "/api/v1/entities/{entity_id}/media/{media_id}"
);

spec_path!(assertions_update, put, "/api/v1/assertions/{id}");

spec_path!(staging_list, get, "/api/v1/staging");
spec_path!(staging_submit, post, "/api/v1/staging");
spec_path!(staging_bulk, post, "/api/v1/staging/bulk");
spec_path!(staging_get, get, "/api/v1/staging/{id}");
spec_path!(staging_approve, post, "/api/v1/staging/{id}/approve");
spec_path!(staging_reject, post, "/api/v1/staging/{id}/reject");

spec_path!(import_start, post, "/api/v1/import");
spec_path!(import_status, get, "/api/v1/import/{job_id}");
spec_path!(export_get, get, "/api/v1/export");

#[derive(OpenApi)]
#[openapi(
    paths(
        health_get,
        persons_list, persons_create, persons_get, persons_update, persons_delete,
        persons_assertions_list, persons_assertions_create, persons_assertions_update,
        persons_timeline, persons_families,
        families_list, families_create, families_get, families_update, families_delete,
        families_assertions,
        events_list, events_create, events_get, events_update, events_delete,
        events_assertions_list, events_assertions_create,
        events_participants_add, events_participants_remove, events_stream,
        search_get,
        graph_ancestors, graph_descendants, graph_pedigree, graph_path, graph_network,
        sources_list, sources_create, sources_get, sources_update, sources_delete,
        citations_list, citations_create, citations_get, citations_update, citations_delete,
        repositories_list, repositories_create, repositories_get, repositories_update, repositories_delete,
        notes_list, notes_create, notes_get, notes_update, notes_delete,
        research_log_list, research_log_create, research_log_get, research_log_update, research_log_delete,
        media_list, media_upload, media_get, media_delete, media_links, media_update_text,
        media_tags_add, media_tags_remove, media_file, media_thumbnail,
        media_extract_get, media_extract_post,
        media_albums_list, media_albums_create, media_album_items_add,
        entity_media_list, entity_media_link, entity_media_unlink,
        assertions_update,
        staging_list, staging_submit, staging_bulk, staging_get, staging_approve, staging_reject,
        import_start, import_status, export_get
    ),
    tags(
        (name = "rustygene", description = "RustyGene local REST API")
    )
)]
pub struct ApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

pub fn router() -> Router<AppState> {
    #[cfg(debug_assertions)]
    {
        Router::new().merge(
            utoipa_swagger_ui::SwaggerUi::new("/api/v1/docs")
                .url("/api/v1/openapi.json", ApiDoc::openapi()),
        )
    }

    #[cfg(not(debug_assertions))]
    {
        Router::new().route("/api/v1/openapi.json", axum::routing::get(openapi_json))
    }
}

#[allow(dead_code)]
async fn openapi_json(State(_state): State<AppState>) -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}
