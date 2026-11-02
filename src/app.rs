use axum::Router;

use crate::{
    config::{Config, cors},
    features,
    state::AppState,
};

pub fn build(config: &Config, state: AppState) -> Router {
    Router::new()
        .nest("/api/v1", features::routes())
        .merge(features::auth::routes())
        .layer(cors::layer(&config.cors_allowed_origins))
        .with_state(state)
}
