use axum::{Json, extract::State, http::StatusCode};

use super::{
    model::{CreatePerusahaan, Perusahaan},
    repository,
};
use crate::{
    error::AppError,
    features::auth::{User, find_peran},
    state::AppState,
};

pub async fn list(
    State(state): State<AppState>,
    user: User,
) -> Result<Json<Vec<Perusahaan>>, AppError> {
    find_peran(&state.db, user.id_pengguna)
        .await?
        .ok_or(AppError::Forbidden)?;

    Ok(Json(repository::list(&state.db).await?))
}

pub async fn create(
    State(state): State<AppState>,
    Json(data): Json<CreatePerusahaan>,
) -> Result<(StatusCode, Json<Perusahaan>), AppError> {
    let data = data.validate()?;

    let perusahaan = repository::create(&state.db, &data.nama_perusahaan)
        .await?
        .ok_or(AppError::Conflict("Perusahaan dengan nama ini sudah ada."))?;

    Ok((StatusCode::CREATED, Json(perusahaan)))
}
