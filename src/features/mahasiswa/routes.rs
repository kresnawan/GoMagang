use axum::{Router, middleware::from_fn_with_state, routing::get};

use super::{handler, middleware::require_mahasiswa};
use crate::state::AppState;

/// Router endpoint data diri mahasiswa: `GET` dan `PATCH /mahasiswa/data`.
///
/// Keduanya dilindungi middleware [`require_mahasiswa`], sehingga hanya mahasiswa yang sudah login
/// yang bisa mengakses.
pub fn routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/mahasiswa/data",
            get(handler::data).patch(handler::update_data),
        )
        .route_layer(from_fn_with_state(state.clone(), require_mahasiswa))
}
