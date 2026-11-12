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

/// `GET /api/v1/periode`: daftar periode pendaftaran, dari yang paling baru.
///
/// Panitia Magang bisa memfilter dengan `id` dan `status` apa pun (`belum_dimulai`, `berlangsung`,
/// `selesai`). Mahasiswa hanya boleh melihat periode yang sedang berlangsung: tanpa filter status,
/// hanya periode berlangsung yang dikembalikan, dan filter status lain dijawab `403`. Pengguna
/// tanpa
/// peran dijawab `403`.
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
        (Peran::PanitiaMagang, status) => status,
        (Peran::Mahasiswa, None | Some(StatusPeriode::Berlangsung)) => {
            Some(StatusPeriode::Berlangsung)
        }
        (Peran::Mahasiswa, Some(_)) => return Err(AppError::Forbidden),
    };

    Ok(Json(repository::list(&state.db, query.id, status).await?))
}

/// `POST /api/v1/periode`: membuat periode pendaftaran baru (hanya Panitia Magang).
///
/// Body divalidasi lebih dulu (lihat [`CreatePeriode::validate`]). Rentang waktu yang beririsan,
/// termasuk yang hanya bersentuhan di ujungnya, dengan periode lain dijawab `409`. Bila berhasil,
/// dijawab `201` beserta periode yang tersimpan dan statusnya saat ini.
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

/// `PATCH /api/v1/periode/{id}/start`: memulai periode yang belum dimulai saat itu juga.
///
/// Waktu mulai dimajukan ke waktu sekarang, sedangkan waktu tutup tetap. Dijawab `404` bila periode
/// tidak ada, dan `409` bila periode sudah berlangsung, sudah selesai, atau rentang barunya akan
/// beririsan dengan periode lain. Bila berhasil, periode terbaru dikembalikan.
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

/// `PATCH /api/v1/periode/{id}/stop`: menghentikan periode yang sedang berlangsung saat itu juga.
///
/// Waktu tutup dimundurkan ke waktu sekarang, sedangkan waktu mulai tetap. Dijawab `404` bila
/// periode tidak ada, dan `409` bila periode belum dimulai atau sudah selesai. Bila berhasil,
/// periode terbaru dikembalikan.
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
