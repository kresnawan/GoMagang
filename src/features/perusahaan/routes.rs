use axum::{
    Router,
    middleware::from_fn_with_state,
    routing::{delete, get, post},
};

use super::handler;
use crate::{features::auth::require_panitia_magang, state::AppState};

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
