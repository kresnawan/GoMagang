use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use super::{
    model::{
        HapusResult, ListQuery, MataKuliah, MataKuliahBaru, Penyetaraan, PenyetaraanBaru,
        TambahResult,
    },
    repository,
};
use crate::{
    error::AppError,
    features::auth::{Peran, User, find_peran},
    state::AppState,
};

const BUKAN_AKTIF: AppError =
    AppError::Conflict("Konversi mata kuliah hanya untuk pendaftaran berstatus Aktif.");

/// The mata kuliah that are not hidden. Panitia Magang may ask for the hidden ones too.
pub async fn list_mk(
    State(state): State<AppState>,
    user: User,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<MataKuliah>>, AppError> {
    let peran = find_peran(&state.db, user.id_pengguna)
        .await?
        .ok_or(AppError::Forbidden)?;

    let termasuk_dihapus = query.termasuk_dihapus.unwrap_or(false);
    if termasuk_dihapus && !matches!(peran, Peran::PanitiaMagang) {
        return Err(AppError::Forbidden);
    }

    Ok(Json(
        repository::list_mk(&state.db, termasuk_dihapus).await?,
    ))
}

pub async fn create_mk(
    State(state): State<AppState>,
    Json(data): Json<MataKuliahBaru>,
) -> Result<(StatusCode, Json<MataKuliah>), AppError> {
    let data = data.validate()?;

    let mata_kuliah = repository::create_mk(&state.db, &data.mk_asal, &data.mk_konversi, data.sks)
        .await?
        .ok_or(AppError::Conflict(
            "Mata kuliah dengan MK asal dan MK konversi ini sudah ada.",
        ))?;

    Ok((StatusCode::CREATED, Json(mata_kuliah)))
}

/// Hides the mata kuliah. Its row stays, so the penyetaraan that chose it keep their history.
pub async fn soft_delete_mk(
    State(state): State<AppState>,
    Path(id_mk_konversi): Path<i32>,
) -> Result<Json<MataKuliah>, AppError> {
    let mata_kuliah = repository::soft_delete_mk(&state.db, id_mk_konversi)
        .await?
        .ok_or(AppError::NotFound)?;

    Ok(Json(mata_kuliah))
}

/// The mata kuliah chosen for a pendaftaran: any for Panitia Magang, only their own for a mahasiswa.
pub async fn list_penyetaraan(
    State(state): State<AppState>,
    user: User,
    Path(id_pendaftaran): Path<i32>,
) -> Result<Json<Vec<Penyetaraan>>, AppError> {
    let peran = find_peran(&state.db, user.id_pengguna)
        .await?
        .ok_or(AppError::Forbidden)?;

    let pemilik = repository::find_pemilik(&state.db, id_pendaftaran)
        .await?
        .ok_or(AppError::NotFound)?;

    if matches!(peran, Peran::Mahasiswa) && pemilik != user.id_pengguna {
        return Err(AppError::Forbidden);
    }

    Ok(Json(
        repository::list_penyetaraan(&state.db, id_pendaftaran).await?,
    ))
}

pub async fn tambah_penyetaraan(
    State(state): State<AppState>,
    user: User,
    Path(id_pendaftaran): Path<i32>,
    Json(data): Json<PenyetaraanBaru>,
) -> Result<(StatusCode, Json<Penyetaraan>), AppError> {
    let hasil = repository::tambah_penyetaraan(
        &state.db,
        id_pendaftaran,
        user.id_pengguna,
        data.id_mk_konversi,
    )
    .await?;

    match hasil {
        TambahResult::Ditambahkan(penyetaraan) => Ok((StatusCode::CREATED, Json(penyetaraan))),
        TambahResult::PendaftaranNotFound => Err(AppError::NotFound),
        TambahResult::NotOwner => Err(AppError::Forbidden),
        TambahResult::NotAktif => Err(BUKAN_AKTIF),
        TambahResult::MataKuliahNotFound => {
            Err(AppError::Validation("Mata kuliah tidak ditemukan."))
        }
        TambahResult::SudahAda => Err(AppError::Conflict("Mata kuliah ini sudah ditambahkan.")),
    }
}

pub async fn hapus_penyetaraan(
    State(state): State<AppState>,
    user: User,
    Path(id_penyetaraan): Path<i32>,
) -> Result<StatusCode, AppError> {
    match repository::hapus_penyetaraan(&state.db, id_penyetaraan, user.id_pengguna).await? {
        HapusResult::Dihapus => Ok(StatusCode::NO_CONTENT),
        HapusResult::NotFound => Err(AppError::NotFound),
        HapusResult::NotOwner => Err(AppError::Forbidden),
        HapusResult::NotAktif => Err(BUKAN_AKTIF),
    }
}
