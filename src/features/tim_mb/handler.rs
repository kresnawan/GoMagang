use axum::{Json, extract::State, http::StatusCode};

use super::{
    model::{RegisterTimMb, TimMb},
    repository,
};
use crate::{error::AppError, state::AppState};

const EMAIL_TAKEN: &str = "Email sudah terdaftar.";

pub async fn register(
    State(state): State<AppState>,
    Json(data): Json<RegisterTimMb>,
) -> Result<(StatusCode, Json<TimMb>), AppError> {
    let data = data.validate()?;

    let id_pengguna = repository::register(&state.db, &data)
        .await
        .map_err(|err| match err.as_database_error() {
            // Two requests for the same email can both pass the check at the same time.
            Some(db_err) if db_err.is_unique_violation() => AppError::Conflict(EMAIL_TAKEN),
            _ => err.into(),
        })?
        .ok_or(AppError::Conflict(EMAIL_TAKEN))?;

    Ok((
        StatusCode::CREATED,
        Json(TimMb {
            id_pengguna,
            email: data.email,
            nama: data.nama,
            no_hp: data.no_hp,
        }),
    ))
}
