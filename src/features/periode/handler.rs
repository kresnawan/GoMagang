use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use super::{
    model::{CreatePeriode, Periode, PeriodeQuery, StartResult, StatusPeriode, StopResult},
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
    Query(query): Query<PeriodeQuery>,
) -> Result<Json<Vec<Periode>>, AppError> {
    let peran = find_peran(&state.db, user.id_pengguna)
        .await?
        .ok_or(AppError::Forbidden)?;

    // Mahasiswa only ever sees running periods, so any other id simply finds nothing.
    let status = match (peran, query.status) {
        (Peran::TimMb, status) => status,
        (Peran::Mahasiswa, None | Some(StatusPeriode::Berlangsung)) => {
            Some(StatusPeriode::Berlangsung)
        }
        (Peran::Mahasiswa, Some(_)) => return Err(AppError::Forbidden),
    };

    Ok(Json(repository::list(&state.db, query.id, status).await?))
}

pub async fn create(
    State(state): State<AppState>,
    Json(data): Json<CreatePeriode>,
) -> Result<(StatusCode, Json<Periode>), AppError> {
    let periode = data.validate()?;

    let id = repository::create(&state.db, &periode)
        .await?
        .ok_or(AppError::Conflict(
            "Rentang waktu beririsan dengan periode lain.",
        ))?;

    let created = repository::list(&state.db, Some(id), None)
        .await?
        .pop()
        .ok_or(AppError::Internal)?;

    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn start(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<Periode>, AppError> {
    match repository::start(&state.db, id).await? {
        StartResult::Started => {}
        StartResult::NotFound => return Err(AppError::NotFound),
        StartResult::AlreadyRunning => {
            return Err(AppError::Conflict("Periode ini sudah berlangsung."));
        }
        StartResult::Finished => {
            return Err(AppError::Conflict("Periode ini sudah selesai."));
        }
        StartResult::Overlapping => {
            return Err(AppError::Conflict(
                "Periode tidak bisa dimulai karena beririsan dengan periode lain.",
            ));
        }
    }

    let started = repository::list(&state.db, Some(id), None)
        .await?
        .pop()
        .ok_or(AppError::Internal)?;

    Ok(Json(started))
}

pub async fn stop(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<Periode>, AppError> {
    match repository::stop(&state.db, id).await? {
        StopResult::Stopped => {}
        StopResult::NotFound => return Err(AppError::NotFound),
        StopResult::NotStarted => {
            return Err(AppError::Conflict("Periode ini belum dimulai."));
        }
        StopResult::Finished => {
            return Err(AppError::Conflict("Periode ini sudah selesai."));
        }
    }

    let stopped = repository::list(&state.db, Some(id), None)
        .await?
        .pop()
        .ok_or(AppError::Internal)?;

    Ok(Json(stopped))
}
