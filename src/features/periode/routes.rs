use axum::{
    Router,
    middleware::from_fn_with_state,
    routing::{get, patch, post},
};

use super::handler;
use crate::{features::auth::require_tim_mb, state::AppState};

pub fn routes(state: &AppState) -> Router<AppState> {
    let create =
        post(handler::create).route_layer(from_fn_with_state(state.clone(), require_tim_mb));

    let start =
        patch(handler::start).route_layer(from_fn_with_state(state.clone(), require_tim_mb));

    let stop = patch(handler::stop).route_layer(from_fn_with_state(state.clone(), require_tim_mb));

    Router::new()
        .route("/periode", get(handler::list).merge(create))
        .route("/periode/{id}/start", start)
        .route("/periode/{id}/stop", stop)
}
