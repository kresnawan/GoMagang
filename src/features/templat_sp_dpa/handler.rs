use axum::{Json, extract::State};

use super::{
    model::{SimpanTemplat, TemplatSpDpa},
    repository,
};
use crate::{
    error::AppError,
    features::auth::{User, find_peran},
    state::AppState,
};

/// `GET /api/v1/templat-sp-dpa`: tautan templat surat pernyataan DPA yang sedang berlaku.
///
/// Bisa diakses mahasiswa dan Panitia Magang; pengguna tanpa peran dijawab `403`. Mengembalikan
/// `null` bila Panitia Magang belum pernah mengatur tautan.
pub async fn find(
    State(state): State<AppState>,
    user: User,
) -> Result<Json<Option<TemplatSpDpa>>, AppError> {
    find_peran(&state.db, user.id_pengguna)
        .await?
        .ok_or(AppError::Forbidden)?;

    Ok(Json(repository::find(&state.db).await?))
}

/// `PUT /api/v1/templat-sp-dpa`: mengganti tautan templat yang berlaku (hanya Panitia Magang).
///
/// Tautan divalidasi lebih dulu (lihat [`SimpanTemplat::validate`]), lalu disimpan sebagai baris
/// baru. Tautan lama tetap ada di tabel sebagai riwayat. Mengembalikan templat yang baru berlaku.
pub async fn simpan(
    State(state): State<AppState>,
    Json(data): Json<SimpanTemplat>,
) -> Result<Json<TemplatSpDpa>, AppError> {
    let data = data.validate()?;

    Ok(Json(
        repository::simpan(&state.db, &data.tautan_templat).await?,
    ))
}
