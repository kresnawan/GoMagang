use axum::Router;

use crate::{
    config::{Config, cors},
    features, logging,
    state::AppState,
};

/// Menyusun router utama aplikasi.
///
/// Semua endpoint fitur dipasang di bawah prefix `/api/v1`, sedangkan endpoint autentikasi
/// (`/auth/...`) dipasang di root karena URL callback-nya didaftarkan di Google. Health check
/// (`/health`) juga di root karena dipakai monitoring, bukan frontend. Layer CORS dipasang di
/// paling luar dengan daftar origin dari konfigurasi. Di luarnya lagi ada logging request dan
/// request ID (lihat [`logging::layer`]), sehingga preflight CORS pun tercatat. Terakhir, `state`
/// dibagikan ke semua handler.
pub fn build(config: &Config, state: AppState) -> Router {
    let router = Router::new()
        .nest("/api/v1", features::routes(&state))
        .merge(features::auth::routes())
        .merge(features::health::routes())
        .layer(cors::layer(&config.cors_allowed_origins));

    logging::layer(router).with_state(state)
}
