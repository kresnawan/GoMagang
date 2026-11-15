pub mod auth;
pub mod health;
mod konversi_mk;
mod mahasiswa;
mod panitia_magang;
pub mod pendaftaran;
mod periode;
mod perusahaan;
mod posisi_magang;
pub mod surat_keterangan;
mod templat_sp_dpa;

use axum::Router;

use crate::state::AppState;

/// Menggabungkan router semua fitur yang dipasang di bawah `/api/v1`.
///
/// `state` diteruskan ke fitur yang memasang middleware autentikasi (misalnya
/// `require_panitia_magang` atau `require_mahasiswa`), karena middleware tersebut membutuhkan
/// pengelola sesi dan database.
pub fn routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .merge(pendaftaran::routes(state))
        .merge(mahasiswa::routes(state))
        .merge(posisi_magang::routes(state))
        .merge(surat_keterangan::routes())
        .merge(periode::routes(state))
        .merge(perusahaan::routes(state))
        .merge(konversi_mk::routes(state))
        .merge(templat_sp_dpa::routes(state))
        .merge(panitia_magang::routes(state))
}
