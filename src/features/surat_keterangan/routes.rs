use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};

use super::handler;
use crate::state::AppState;

/// Router endpoint surat keterangan kepesertaan magang.
///
/// - `GET /pendaftaran/{id}/surat-keterangan`: unduh surat; login wajib, aturan peran ada di
///   handler.
/// - `GET /surat-keterangan/verifikasi` dan `POST /surat-keterangan/verifikasi`: publik tanpa
///   login,
///   karena siapa pun yang memegang surat harus bisa memeriksa keasliannya. Unggahan PDF dibatasi
///   `BATAS_BODY_VERIFIKASI`.
/// - `GET /surat-keterangan/kunci-publik`: publik.
pub fn routes() -> Router<AppState> {
    let verifikasi_berkas = post(handler::verifikasi_berkas)
        .route_layer(DefaultBodyLimit::max(handler::BATAS_BODY_VERIFIKASI));

    Router::new()
        .route("/pendaftaran/{id}/surat-keterangan", get(handler::unduh))
        // Both verification routes are public: whoever holds a surat must be able to check it.
        .route(
            "/surat-keterangan/verifikasi",
            get(handler::verifikasi).merge(verifikasi_berkas),
        )
        .route("/surat-keterangan/kunci-publik", get(handler::kunci_publik))
}
