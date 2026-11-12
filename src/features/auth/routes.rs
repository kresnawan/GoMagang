use axum::{
    Router,
    routing::{get, post},
};

use super::handler;
use crate::state::AppState;

/// Router endpoint autentikasi di bawah `/auth`.
///
/// Endpoint ini dipasang di root, bukan di `/api/v1`, karena URL callback `/auth/google` sudah
/// didaftarkan di Google Cloud Console dan cookie refresh token terikat ke path `/auth`.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/google/login", get(handler::google_login))
        .route("/auth/google", get(handler::google_callback))
        .route("/auth/me", get(handler::me))
        .route("/auth/refresh", post(handler::refresh))
        .route("/auth/logout", post(handler::logout))
}
