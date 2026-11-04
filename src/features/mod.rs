pub mod auth;
mod mahasiswa;
mod pendaftaran;
mod periode;
mod perusahaan;
mod posisi_magang;
mod templat_sp_dpa;
mod tim_mb;

use axum::Router;

use crate::state::AppState;

pub fn routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .merge(pendaftaran::routes())
        .merge(mahasiswa::routes(state))
        .merge(posisi_magang::routes(state))
        .merge(periode::routes(state))
        .merge(perusahaan::routes(state))
        .merge(templat_sp_dpa::routes())
        .merge(tim_mb::routes(state))
}
