use axum::{Router, middleware::from_fn_with_state, routing::post};

use super::{handler, middleware::require_admin_secret};
use crate::state::AppState;

/// The route only exists when `ADMIN_SECRET` is set.
pub fn routes(state: &AppState) -> Router<AppState> {
    if state.admin_secret.is_none() {
        return Router::new();
    }

    Router::new()
        .route("/tim-mb", post(handler::register))
        .route_layer(from_fn_with_state(state.clone(), require_admin_secret))
}
