use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use super::{
    model::{ListQuery, NamaPerusahaan, PerusahaanMitra, RenameResult},
    repository,
};
use crate::{
    error::AppError,
    features::auth::{Peran, User, find_peran},
    state::AppState,
};

const NAMA_DIPAKAI: AppError = AppError::Conflict("Perusahaan dengan nama ini sudah ada.");

/// `GET /api/v1/perusahaan`: daftar perusahaan mitra, diurutkan berdasarkan nama.
///
/// Secara default hanya perusahaan yang tidak disembunyikan yang dikembalikan. Panitia Magang boleh
/// meminta perusahaan yang disembunyikan juga dengan `termasuk_dihapus=true`; permintaan itu dari
/// mahasiswa dijawab `403`.
pub async fn list(
    State(state): State<AppState>,
    user: User,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<PerusahaanMitra>>, AppError> {
    let peran = find_peran(&state.db, user.id_pengguna)
        .await?
        .ok_or(AppError::Forbidden)?;

    let termasuk_dihapus = query.termasuk_dihapus.unwrap_or(false);
    if termasuk_dihapus && !matches!(peran, Peran::PanitiaMagang) {
        return Err(AppError::Forbidden);
    }

    Ok(Json(repository::list(&state.db, termasuk_dihapus).await?))
}

/// `POST /api/v1/perusahaan`: menambah perusahaan mitra (hanya Panitia Magang).
///
/// Nama dirapikan dan divalidasi lebih dulu. Nama yang sudah dipakai perusahaan lain yang tidak
/// disembunyikan, tanpa membedakan huruf besar-kecil, dijawab `409`. Bila berhasil, dijawab `201`.
pub async fn create(
    State(state): State<AppState>,
    Json(data): Json<NamaPerusahaan>,
) -> Result<(StatusCode, Json<PerusahaanMitra>), AppError> {
    let data = data.validate()?;

    let perusahaan = repository::create(&state.db, &data.nama_perusahaan)
        .await?
        .ok_or(NAMA_DIPAKAI)?;

    Ok((StatusCode::CREATED, Json(perusahaan)))
}

/// `PATCH /api/v1/perusahaan/{id}`: mengganti nama perusahaan mitra (hanya Panitia Magang).
///
/// Perusahaan yang tidak ada atau sudah disembunyikan dijawab `404`. Nama yang sudah dipakai
/// perusahaan lain yang tidak disembunyikan dijawab `409`.
pub async fn rename(
    State(state): State<AppState>,
    Path(id_perusahaan): Path<i32>,
    Json(data): Json<NamaPerusahaan>,
) -> Result<Json<PerusahaanMitra>, AppError> {
    let data = data.validate()?;

    match repository::rename(&state.db, id_perusahaan, &data.nama_perusahaan).await? {
        RenameResult::Renamed(perusahaan) => Ok(Json(perusahaan)),
        RenameResult::NotFound => Err(AppError::NotFound),
        RenameResult::NameTaken => Err(NAMA_DIPAKAI),
    }
}

/// `DELETE /api/v1/perusahaan/{id}`: menyembunyikan perusahaan mitra (soft delete, hanya Panitia
/// Magang).
///
/// Baris perusahaan tetap ada dengan waktu disembunyikan di `deleted_at`, sehingga posisi dan
/// pendaftaran yang terkait tetap punya riwayat. Perusahaan yang disembunyikan tidak ditawarkan
/// lagi
/// untuk posisi dan pendaftaran baru. Perusahaan yang tidak ada atau sudah disembunyikan dijawab
/// `404`.
pub async fn soft_delete(
    State(state): State<AppState>,
    Path(id_perusahaan): Path<i32>,
) -> Result<Json<PerusahaanMitra>, AppError> {
    let perusahaan = repository::soft_delete(&state.db, id_perusahaan)
        .await?
        .ok_or(AppError::NotFound)?;

    Ok(Json(perusahaan))
}
