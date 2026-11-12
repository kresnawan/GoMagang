use axum::{
    Router,
    middleware::from_fn_with_state,
    routing::{delete, get, post},
};

use super::handler;
use crate::{
    features::{auth::require_panitia_magang, mahasiswa::require_mahasiswa},
    state::AppState,
};

/// Router endpoint konversi mata kuliah.
///
/// - `GET /mata-kuliah` dan `GET /pendaftaran/{id}/penyetaraan-mk`: semua pengguna yang login
///   (aturan per peran ada di handler).
/// - `POST /mata-kuliah` dan `DELETE /mata-kuliah/{id}`: hanya Panitia Magang.
/// - `POST /pendaftaran/{id}/penyetaraan-mk` dan `DELETE /penyetaraan-mk/{id}`: hanya mahasiswa.
pub fn routes(state: &AppState) -> Router<AppState> {
    let create_mk = post(handler::create_mk)
        .route_layer(from_fn_with_state(state.clone(), require_panitia_magang));

    let hapus_mk = delete(handler::soft_delete_mk)
        .route_layer(from_fn_with_state(state.clone(), require_panitia_magang));

    let tambah_penyetaraan = post(handler::tambah_penyetaraan)
        .route_layer(from_fn_with_state(state.clone(), require_mahasiswa));

    let hapus_penyetaraan = delete(handler::hapus_penyetaraan)
        .route_layer(from_fn_with_state(state.clone(), require_mahasiswa));

    Router::new()
        .route("/mata-kuliah", get(handler::list_mk).merge(create_mk))
        .route("/mata-kuliah/{id}", hapus_mk)
        .route(
            "/pendaftaran/{id}/penyetaraan-mk",
            get(handler::list_penyetaraan).merge(tambah_penyetaraan),
        )
        .route("/penyetaraan-mk/{id}", hapus_penyetaraan)
}
