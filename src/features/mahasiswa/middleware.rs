use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};

use super::repository;
use crate::{error::AppError, features::auth::User, state::AppState};

/// Middleware yang hanya meloloskan pengguna yang punya baris di tabel `mahasiswa`.
///
/// Extractor [`User`] menjawab `401` bila access token tidak ada atau tidak valid; jawaban ini yang
/// memberi tahu frontend untuk me-refresh sesi. Token yang valid tetapi bukan milik mahasiswa
/// (misalnya Panitia Magang) dijawab `403`.
///
/// Data diri mahasiswa yang ditemukan disisipkan ke extension request, sehingga handler bisa
/// mengambilnya dengan `Extension<Mahasiswa>` tanpa query ulang.
pub async fn require_mahasiswa(
    State(state): State<AppState>,
    user: User,
    mut request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let mahasiswa = repository::find_by_id_pengguna(&state.db, user.id_pengguna)
        .await?
        .ok_or(AppError::Forbidden)?;
    request.extensions_mut().insert(mahasiswa);

    Ok(next.run(request).await)
}
