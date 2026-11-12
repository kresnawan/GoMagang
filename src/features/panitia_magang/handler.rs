use axum::{Json, extract::State, http::StatusCode};

use super::{
    model::{PanitiaMagang, RegisterPanitiaMagang},
    repository,
};
use crate::{error::AppError, state::AppState};

const EMAIL_TAKEN: &str = "Email sudah terdaftar.";

/// `POST /api/v1/panitia-magang`: mendaftarkan anggota Panitia Magang baru (endpoint sementara).
///
/// Hanya bisa dipanggil dengan header `X-Admin-Secret` yang benar. Email harus alamat `@gmail.com`
/// dan dibandingkan tanpa membedakan huruf besar-kecil. Anggota yang didaftarkan baru bisa login
/// setelah ini, dan akun Google-nya diikat pada login pertama.
///
/// Dijawab `201` beserta data yang tersimpan. Email yang sudah dipakai pengguna mana pun dijawab
/// `409`, termasuk bila dua request dengan email yang sama datang bersamaan dan keduanya lolos
/// pengecekan awal; dalam kasus itu, constraint unik di database yang menangkapnya.
pub async fn register(
    State(state): State<AppState>,
    Json(data): Json<RegisterPanitiaMagang>,
) -> Result<(StatusCode, Json<PanitiaMagang>), AppError> {
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
        Json(PanitiaMagang {
            id_pengguna,
            email: data.email,
            nama: data.nama,
            no_hp: data.no_hp,
        }),
    ))
}
