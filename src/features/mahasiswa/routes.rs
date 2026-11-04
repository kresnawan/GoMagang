use axum::{Router, middleware::from_fn_with_state, routing::get};

use super::{handler, middleware::require_mahasiswa};
use crate::state::AppState;

pub fn routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/mahasiswa/data",
            get(handler::data).patch(handler::update_data),
        )
        .route_layer(from_fn_with_state(state.clone(), require_mahasiswa))
}
