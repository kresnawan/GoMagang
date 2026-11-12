use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};

use super::{
    model::{Peran, User},
    repository::find_peran,
};
use crate::{error::AppError, state::AppState};

/// Middleware yang hanya meloloskan pengguna yang terdaftar sebagai Panitia Magang.
///
/// Extractor [`User`] lebih dulu memeriksa cookie `access_token`; bila tidak ada atau tidak valid,
/// request dijawab `401` sebelum middleware ini berjalan. Setelah itu peran pengguna dicek di
/// database lewat [`find_peran`]. Hanya pengguna yang punya baris di tabel `panitia_magang` yang
/// diteruskan ke handler; selain itu, termasuk mahasiswa, dijawab `403`.
pub async fn require_panitia_magang(
    State(state): State<AppState>,
    user: User,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    match find_peran(&state.db, user.id_pengguna).await? {
        Some(Peran::PanitiaMagang) => Ok(next.run(request).await),
        _ => Err(AppError::Forbidden),
    }
}
