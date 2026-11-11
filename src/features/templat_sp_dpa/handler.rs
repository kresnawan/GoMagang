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

/// The template in use, or `null` while Panitia Magang has not set one. For mahasiswa and Panitia Magang.
pub async fn find(
    State(state): State<AppState>,
    user: User,
) -> Result<Json<Option<TemplatSpDpa>>, AppError> {
    find_peran(&state.db, user.id_pengguna)
        .await?
        .ok_or(AppError::Forbidden)?;

    Ok(Json(repository::find(&state.db).await?))
}

/// Replaces the template in use. The old link stays in the table as history.
pub async fn simpan(
    State(state): State<AppState>,
    Json(data): Json<SimpanTemplat>,
) -> Result<Json<TemplatSpDpa>, AppError> {
    let data = data.validate()?;

    Ok(Json(
        repository::simpan(&state.db, &data.tautan_templat).await?,
    ))
}
