use axum::{
    Router,
    middleware::from_fn_with_state,
    routing::{delete, get, post},
};

use super::handler;
use crate::{features::auth::require_panitia_magang, state::AppState};

/// Router endpoint perusahaan mitra.
///
/// `GET /perusahaan` bisa diakses semua pengguna yang login. `POST /perusahaan`,
/// `PATCH /perusahaan/{id}`, dan `DELETE /perusahaan/{id}` hanya untuk Panitia Magang lewat
/// middleware `require_panitia_magang`.
pub fn routes(state: &AppState) -> Router<AppState> {
    let create = post(handler::create)
        .route_layer(from_fn_with_state(state.clone(), require_panitia_magang));

    let ubah = delete(handler::soft_delete)
        .patch(handler::rename)
        .route_layer(from_fn_with_state(state.clone(), require_panitia_magang));

    Router::new()
        .route("/perusahaan", get(handler::list).merge(create))
        .route("/perusahaan/{id}", ubah)
}
