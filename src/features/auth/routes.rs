use axum::{
    Router,
    routing::{get, post},
};

use super::handler;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/google/login", get(handler::google_login))
        .route("/auth/google", get(handler::google_callback))
        .route("/auth/me", get(handler::me))
        .route("/auth/refresh", post(handler::refresh))
        .route("/auth/logout", post(handler::logout))
}
