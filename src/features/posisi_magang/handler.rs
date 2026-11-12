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

/// `GET /api/v1/posisi`: daftar posisi magang dalam satu periode.
///
/// Periode yang dipakai:
/// - Mahasiswa: hanya periode yang sedang berlangsung. Tanpa `id_periode`, dipakai periode
///   berlangsung yang paling baru. `id_periode` lain dijawab `403`.
/// - Panitia Magang: periode mana pun. Tanpa `id_periode`, dipakai periode yang sedang berlangsung,
///   atau periode terbaru bila tidak ada yang berlangsung.
///
/// Bila tidak ada periode yang bisa dipakai, dikembalikan daftar kosong. Filter `p` (id perusahaan)
/// dan `s` (pencarian nama posisi) bersifat opsional. Posisi yang disembunyikan hanya bisa diminta
/// Panitia Magang lewat `termasuk_dihapus=true`, karena mahasiswa tidak bisa mendaftar ke posisi
/// tersebut.
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

/// `POST /api/v1/posisi`: menambah posisi magang (hanya Panitia Magang).
///
/// Body divalidasi lebih dulu (lihat [`CreatePosisi::validate`]). Periode yang tidak ada dijawab
/// `422` "Periode tidak ditemukan.", yang dikenali dari pelanggaran foreign key. Perusahaan yang
/// tidak
/// ada atau sudah disembunyikan dijawab `422` "Perusahaan tidak ditemukan.", karena perusahaan
/// tersembunyi tidak menerima posisi baru. Bila berhasil, dijawab `201` beserta posisi dan
/// perusahaannya.
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

/// `PATCH /api/v1/posisi/{id}`: mengubah nama, deskripsi persyaratan, dan kuota posisi (hanya
/// Panitia Magang).
///
/// Perusahaan dan periode posisi tidak bisa diubah, karena pendaftaran pada posisi itu terikat ke
/// keduanya. Posisi yang tidak ada atau disembunyikan, maupun yang perusahaannya disembunyikan,
/// dijawab `404`.
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

/// `DELETE /api/v1/posisi/{id}`: menyembunyikan posisi magang (soft delete, hanya Panitia Magang).
///
/// Baris posisi tetap ada dengan waktu disembunyikan di `deleted_at`, sehingga pendaftaran pada
/// posisi itu tetap punya riwayat. Posisi yang disembunyikan tidak tampil di katalog mahasiswa dan
/// tidak bisa didaftar. Posisi yang tidak ada, sudah disembunyikan, atau perusahaannya
/// disembunyikan
/// dijawab `404`.
pub async fn sembunyikan(
    State(state): State<AppState>,
    Path(id_posisi): Path<i32>,
) -> Result<Json<PosisiMagang>, AppError> {
    let posisi = repository::sembunyikan(&state.db, id_posisi)
        .await?
        .ok_or(AppError::NotFound)?;

    Ok(Json(posisi))
}
