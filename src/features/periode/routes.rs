use axum::{
    Router,
    middleware::from_fn_with_state,
    routing::{get, patch, post},
};

use super::handler;
use crate::{features::auth::require_panitia_magang, state::AppState};

pub fn routes(state: &AppState) -> Router<AppState> {
    let create = post(handler::create)
        .route_layer(from_fn_with_state(state.clone(), require_panitia_magang));

    let start = patch(handler::start)
        .route_layer(from_fn_with_state(state.clone(), require_panitia_magang));

    let stop =
        patch(handler::stop).route_layer(from_fn_with_state(state.clone(), require_panitia_magang));

    Router::new()
        .route("/periode", get(handler::list).merge(create))
        .route("/periode/{id}/start", start)
        .route("/periode/{id}/stop", stop)
}
