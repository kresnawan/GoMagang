use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware::from_fn_with_state,
    routing::{get, patch, post, put},
};

use super::{handler, model::MAX_BODY_SIZE};
use crate::{
    features::{auth::require_panitia_magang, mahasiswa::require_mahasiswa},
    state::AppState,
};

pub fn routes(state: &AppState) -> Router<AppState> {
    let create = post(handler::create)
        .route_layer(DefaultBodyLimit::max(MAX_BODY_SIZE))
        .route_layer(from_fn_with_state(state.clone(), require_mahasiswa));

    let update_status = patch(handler::update_status)
        .route_layer(from_fn_with_state(state.clone(), require_panitia_magang));

    let accept =
        patch(handler::accept).route_layer(from_fn_with_state(state.clone(), require_mahasiswa));

    let simpan_jadwal = put(handler::simpan_jadwal)
        .route_layer(from_fn_with_state(state.clone(), require_panitia_magang));

    let simpan_plotting = put(handler::simpan_plotting)
        .route_layer(from_fn_with_state(state.clone(), require_panitia_magang));

    let ekspor = post(handler::ekspor)
        .route_layer(from_fn_with_state(state.clone(), require_panitia_magang));

    Router::new()
        .route(
            "/pendaftaran",
            get(handler::list).merge(create).merge(update_status),
        )
        .route("/pendaftaran/accept", accept)
        .route("/pendaftaran/jadwal-seleksi-substansi", simpan_jadwal)
        .route("/pendaftaran/plot-dpm", simpan_plotting)
        .route("/pendaftaran/ekspor", ekspor)
        .route("/pendaftaran/{id}", get(handler::detail))
        .route("/berkas/{id_berkas}", get(handler::download_berkas))
        .route(
            "/berkas/publik/{token}",
            get(handler::download_berkas_publik),
        )
}
