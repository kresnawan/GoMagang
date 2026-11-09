use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};

use super::handler;
use crate::state::AppState;

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
