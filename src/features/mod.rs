pub mod auth;
mod data_diri;
mod lowongan;
mod pendaftaran;
mod periode;
mod templat_sp_dpa;

use axum::Router;

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .merge(pendaftaran::routes())
        .merge(data_diri::routes())
        .merge(lowongan::routes())
        .merge(periode::routes())
        .merge(templat_sp_dpa::routes())
}
