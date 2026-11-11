use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use super::{
    model::{CreatePosisi, PosisiMagang, PosisiQuery, UbahPosisi},
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

    // Only Panitia Magang may see the hidden positions, as mahasiswa cannot register for them.
    let termasuk_dihapus = query.termasuk_dihapus.unwrap_or(false);
    if termasuk_dihapus && !matches!(peran, Peran::PanitiaMagang) {
        return Err(AppError::Forbidden);
    }

    let id_periode = match peran {
        Peran::Mahasiswa => {
            let running = repository::running_periode_ids(&state.db).await?;
            match query.id_periode {
                Some(id) if running.contains(&id) => Some(id),
                Some(_) => return Err(AppError::Forbidden),
                None => running.first().copied(),
            }
        }
        Peran::PanitiaMagang => match query.id_periode {
            Some(id) => Some(id),
            None => repository::default_periode_id(&state.db).await?,
        },
    };

    let Some(id_periode) = id_periode else {
        return Ok(Json(Vec::new()));
    };

    Ok(Json(
        repository::list(&state.db, id_periode, &query, termasuk_dihapus).await?,
    ))
}

pub async fn create(
    State(state): State<AppState>,
    Json(data): Json<CreatePosisi>,
) -> Result<(StatusCode, Json<PosisiMagang>), AppError> {
    let data = data.validate()?;

    let posisi = repository::create(&state.db, &data)
        .await
        .map_err(|err| {
            let constraint = err
                .as_database_error()
                .and_then(|db_err| db_err.constraint());
            match constraint {
                Some("posisi_magang_id_periode_fkey") => {
                    AppError::Validation("Periode tidak ditemukan.")
                }
                _ => err.into(),
            }
        })?
        // Hidden companies take no new positions, so they count as missing here.
        .ok_or(AppError::Validation("Perusahaan tidak ditemukan."))?;

    Ok((StatusCode::CREATED, Json(posisi)))
}

pub async fn ubah(
    State(state): State<AppState>,
    Path(id_posisi): Path<i32>,
    Json(data): Json<UbahPosisi>,
) -> Result<Json<PosisiMagang>, AppError> {
    let data = data.validate()?;

    let posisi = repository::ubah(&state.db, id_posisi, &data)
        .await?
        .ok_or(AppError::NotFound)?;

    Ok(Json(posisi))
}

/// Hides the position. Its row stays, so the pendaftaran on it keep their history.
pub async fn sembunyikan(
    State(state): State<AppState>,
    Path(id_posisi): Path<i32>,
) -> Result<Json<PosisiMagang>, AppError> {
    let posisi = repository::sembunyikan(&state.db, id_posisi)
        .await?
        .ok_or(AppError::NotFound)?;

    Ok(Json(posisi))
}
