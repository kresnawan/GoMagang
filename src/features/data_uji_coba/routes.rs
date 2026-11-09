use axum::{
    Router,
    middleware::from_fn_with_state,
    routing::{delete, post},
};

use super::handler;
use crate::{features::tim_mb::require_admin_secret, state::AppState};

/// The routes only exist when `ADMIN_SECRET` is set.
pub fn routes(state: &AppState) -> Router<AppState> {
    if state.admin_secret.is_none() {
        return Router::new();
    }

    Router::new()
        .route("/mahasiswa", delete(handler::hapus_semua))
        .route("/mahasiswa/seed", post(handler::seed))
        .route_layer(from_fn_with_state(state.clone(), require_admin_secret))
}
