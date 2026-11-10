use axum::{
    Router,
    middleware::from_fn_with_state,
    routing::{delete, get, post},
};

use super::handler;
use crate::{features::auth::require_tim_mb, state::AppState};

pub fn routes(state: &AppState) -> Router<AppState> {
    let create =
        post(handler::create).route_layer(from_fn_with_state(state.clone(), require_tim_mb));

    let ubah = delete(handler::sembunyikan)
        .patch(handler::ubah)
        .route_layer(from_fn_with_state(state.clone(), require_tim_mb));

    Router::new()
        .route("/posisi", get(handler::list).merge(create))
        .route("/posisi/{id}", ubah)
}
