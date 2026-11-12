use axum::{Extension, Json, extract::State};

use super::{
    model::{Mahasiswa, MahasiswaPatch},
    repository,
};
use crate::{error::AppError, features::auth::User, state::AppState};

/// `GET /api/v1/mahasiswa/data`: mengembalikan data diri mahasiswa yang sedang login.
///
/// Datanya sudah dimuat oleh middleware [`require_mahasiswa`](super::middleware::require_mahasiswa)
/// dan diteruskan lewat extension request, sehingga handler ini tidak perlu query ulang ke
/// database.
pub async fn data(Extension(mahasiswa): Extension<Mahasiswa>) -> Json<Mahasiswa> {
    Json(mahasiswa)
}

/// `PATCH /api/v1/mahasiswa/data`: mengubah sebagian data diri mahasiswa yang sedang login.
///
/// Hanya field yang dikirim yang diubah; field lain tetap. Body divalidasi lebih dulu (lihat
/// [`MahasiswaPatch::validate`]). NIM yang sudah dipakai mahasiswa lain dijawab `409`. Bila
/// berhasil, data diri terbaru dikembalikan, termasuk field yang tidak diubah.
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
