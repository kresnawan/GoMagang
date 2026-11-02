use axum::{Router, routing::get};

use super::handler;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/lowongan", get(handler::list))
}
