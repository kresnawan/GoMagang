use axum::{Router, middleware::from_fn_with_state, routing::post};

use super::{handler, middleware::require_admin_secret};
use crate::state::AppState;

/// Router endpoint pendaftaran Panitia Magang: `POST /panitia-magang`.
///
/// Route ini hanya dipasang bila `ADMIN_SECRET` di-set; tanpa secret, endpoint tidak ada sama
/// sekali
/// (`404`). Bila dipasang, route dilindungi middleware [`require_admin_secret`].
pub fn routes(state: &AppState) -> Router<AppState> {
    if state.admin_secret.is_none() {
        return Router::new();
    }

    Router::new()
        .route("/panitia-magang", post(handler::register))
        .route_layer(from_fn_with_state(state.clone(), require_admin_secret))
}
