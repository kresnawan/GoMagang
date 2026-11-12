use axum::{
    Router,
    middleware::from_fn_with_state,
    routing::{get, put},
};

use super::handler;
use crate::{features::auth::require_panitia_magang, state::AppState};

/// Router endpoint templat SP DPA.
///
/// `GET /templat-sp-dpa` untuk semua pengguna yang punya peran, sedangkan `PUT /templat-sp-dpa`
/// hanya untuk Panitia Magang lewat middleware `require_panitia_magang`.
pub fn routes(state: &AppState) -> Router<AppState> {
    let simpan =
        put(handler::simpan).route_layer(from_fn_with_state(state.clone(), require_panitia_magang));

    Router::new().route("/templat-sp-dpa", get(handler::find).merge(simpan))
}
