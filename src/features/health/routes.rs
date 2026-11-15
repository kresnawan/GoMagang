use axum::{Router, routing::get};

use super::handler;
use crate::state::AppState;

/// Router `GET /health`.
///
/// Dipasang di root, bukan di `/api/v1`, karena endpoint ini untuk monitoring (reverse proxy atau
/// uptime checker), bukan bagian dari API yang dipakai frontend. Tidak butuh login.
pub fn routes() -> Router<AppState> {
    Router::new().route("/health", get(handler::health))
}
