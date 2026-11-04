use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};

use super::{
    model::{CreatePosisi, PosisiMagang, PosisiQuery},
    repository,
};
use crate::{
    error::AppError,
    features::auth::{Peran, User, find_peran},
    state::AppState,
};

pub async fn list(
    State(state): State<AppState>,
    user: User,
    Query(query): Query<PosisiQuery>,
) -> Result<Json<Vec<PosisiMagang>>, AppError> {
    let peran = find_peran(&state.db, user.id_pengguna)
        .await?
        .ok_or(AppError::Forbidden)?;

    let id_periode = match peran {
        Peran::Mahasiswa => {
            let running = repository::running_periode_ids(&state.db).await?;
            match query.id_periode {
                Some(id) if running.contains(&id) => Some(id),
                Some(_) => return Err(AppError::Forbidden),
                None => running.first().copied(),
            }
        }
        Peran::TimMb => match query.id_periode {
            Some(id) => Some(id),
            None => repository::default_periode_id(&state.db).await?,
        },
    };

    let Some(id_periode) = id_periode else {
        return Ok(Json(Vec::new()));
    };

    Ok(Json(repository::list(&state.db, id_periode, &query).await?))
}

pub async fn create(
    State(state): State<AppState>,
    Json(data): Json<CreatePosisi>,
) -> Result<(StatusCode, Json<PosisiMagang>), AppError> {
    let data = data.validate()?;

    let posisi = repository::create(&state.db, &data).await.map_err(|err| {
        let constraint = err
            .as_database_error()
            .and_then(|db_err| db_err.constraint());
        match constraint {
            Some("posisi_magang_id_perusahaan_fkey") => {
                AppError::Validation("Perusahaan tidak ditemukan.")
            }
            Some("posisi_magang_id_periode_fkey") => {
                AppError::Validation("Periode tidak ditemukan.")
            }
            _ => err.into(),
        }
    })?;

    Ok((StatusCode::CREATED, Json(posisi)))
}
