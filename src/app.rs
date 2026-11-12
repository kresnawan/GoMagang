use axum::Router;

use crate::{
    config::{Config, cors},
    features,
    state::AppState,
};

/// Menyusun router utama aplikasi.
///
/// Semua endpoint fitur dipasang di bawah prefix `/api/v1`, sedangkan endpoint autentikasi
/// (`/auth/...`) dipasang di root karena URL callback-nya didaftarkan di Google. Layer CORS
/// dipasang
/// di paling luar dengan daftar origin dari konfigurasi, lalu `state` dibagikan ke semua handler.
pub fn build(config: &Config, state: AppState) -> Router {
    Router::new()
        .nest("/api/v1", features::routes(&state))
        .merge(features::auth::routes())
        .layer(cors::layer(&config.cors_allowed_origins))
        .with_state(state)
}
