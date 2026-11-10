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

/// The companies that are not hidden. Tim MB may ask for the hidden ones too.
pub async fn list(
    State(state): State<AppState>,
    user: User,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<PerusahaanMitra>>, AppError> {
    let peran = find_peran(&state.db, user.id_pengguna)
        .await?
        .ok_or(AppError::Forbidden)?;

    let termasuk_dihapus = query.termasuk_dihapus.unwrap_or(false);
    if termasuk_dihapus && !matches!(peran, Peran::TimMb) {
        return Err(AppError::Forbidden);
    }

    Ok(Json(repository::list(&state.db, termasuk_dihapus).await?))
}

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

/// Hides the company. Its row stays, so pendaftaran on its positions keep their history.
pub async fn soft_delete(
    State(state): State<AppState>,
    Path(id_perusahaan): Path<i32>,
) -> Result<Json<PerusahaanMitra>, AppError> {
    let perusahaan = repository::soft_delete(&state.db, id_perusahaan)
        .await?
        .ok_or(AppError::NotFound)?;

    Ok(Json(perusahaan))
}
