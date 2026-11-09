pub mod auth;
mod data_uji_coba;
mod mahasiswa;
pub mod pendaftaran;
mod periode;
mod perusahaan;
mod posisi_magang;
pub mod surat_keterangan;
mod templat_sp_dpa;
mod tim_mb;

use axum::Router;

use crate::state::AppState;

pub fn routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .merge(pendaftaran::routes(state))
        .merge(mahasiswa::routes(state))
        .merge(posisi_magang::routes(state))
        .merge(surat_keterangan::routes())
        .merge(periode::routes(state))
        .merge(perusahaan::routes(state))
        .merge(templat_sp_dpa::routes())
        .merge(tim_mb::routes(state))
        .merge(data_uji_coba::routes(state))
}
