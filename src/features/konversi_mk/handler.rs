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

/// `GET /api/v1/mata-kuliah`: katalog mata kuliah konversi.
///
/// Secara default hanya mata kuliah yang tidak disembunyikan yang dikembalikan. Panitia Magang
/// boleh meminta yang disembunyikan juga dengan `termasuk_dihapus=true`; permintaan itu dari
/// mahasiswa dijawab `403`. Pengguna tanpa peran juga dijawab `403`.
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

/// `POST /api/v1/mata-kuliah`: menambah mata kuliah ke katalog (hanya Panitia Magang).
///
/// Body divalidasi lebih dulu (lihat [`MataKuliahBaru::validate`]). Pasangan MK asal dan MK
/// konversi yang sudah ada di mata kuliah lain yang tidak disembunyikan, tanpa membedakan huruf
/// besar-kecil, dijawab `409`. Bila berhasil, dijawab `201`.
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

/// `DELETE /api/v1/mata-kuliah/{id}`: menyembunyikan mata kuliah dari katalog (soft delete, hanya
/// Panitia Magang).
///
/// Baris mata kuliah tetap ada dengan waktu disembunyikan di `deleted_at`, sehingga penyetaraan
/// yang sudah memilihnya tetap punya riwayat. Mata kuliah yang disembunyikan tidak bisa dipilih
/// lagi. Mata kuliah yang tidak ada atau sudah disembunyikan dijawab `404`.
pub async fn soft_delete_mk(
    State(state): State<AppState>,
    Path(id_mk_konversi): Path<i32>,
) -> Result<Json<MataKuliah>, AppError> {
    let mata_kuliah = repository::soft_delete_mk(&state.db, id_mk_konversi)
        .await?
        .ok_or(AppError::NotFound)?;

    Ok(Json(mata_kuliah))
}

/// `GET /api/v1/pendaftaran/{id}/penyetaraan-mk`: mata kuliah yang dipilih untuk sebuah
/// pendaftaran.
///
/// Panitia Magang bisa melihat pendaftaran mana pun, sedangkan mahasiswa hanya pendaftarannya
/// sendiri; pendaftaran milik orang lain dijawab `403`. Pendaftaran yang tidak ada dijawab `404`.
/// Mata kuliah yang sudah disembunyikan tetap muncul, karena sudah terlanjur dipilih.
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

/// `POST /api/v1/pendaftaran/{id}/penyetaraan-mk`: mahasiswa memilih mata kuliah untuk
/// pendaftarannya.
///
/// Syaratnya: pendaftaran ada (`404`), milik mahasiswa ini (`403`), berstatus `Aktif` (`409`), dan
/// mata kuliahnya ada serta tidak disembunyikan (`422`). Mata kuliah yang sudah dipilih sebelumnya
/// dijawab `409`. Bila berhasil, dijawab `201` beserta penyetaraan baru.
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

/// `DELETE /api/v1/penyetaraan-mk/{id}`: mahasiswa membatalkan pilihan mata kuliah.
///
/// Penyetaraan yang tidak ada dijawab `404`, milik mahasiswa lain `403`, dan pada pendaftaran yang
/// tidak berstatus `Aktif` dijawab `409`. Bila berhasil, dijawab `204` tanpa body.
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
