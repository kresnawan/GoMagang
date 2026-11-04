use axum::{Extension, Json, extract::State};

use super::{
    model::{Mahasiswa, MahasiswaPatch},
    repository,
};
use crate::{error::AppError, features::auth::User, state::AppState};

pub async fn data(Extension(mahasiswa): Extension<Mahasiswa>) -> Json<Mahasiswa> {
    Json(mahasiswa)
}

pub async fn update_data(
    State(state): State<AppState>,
    user: User,
    Json(patch): Json<MahasiswaPatch>,
) -> Result<Json<Mahasiswa>, AppError> {
    let patch = patch.validate()?;

    repository::update(&state.db, user.id_pengguna, patch)
        .await
        .map_err(|err| match err.as_database_error() {
            Some(db_err) if db_err.is_unique_violation() => {
                AppError::Conflict("NIM sudah dipakai mahasiswa lain.")
            }
            _ => err.into(),
        })?;

    let mahasiswa = repository::find_by_id_pengguna(&state.db, user.id_pengguna)
        .await?
        .ok_or(AppError::Forbidden)?;

    Ok(Json(mahasiswa))
}
