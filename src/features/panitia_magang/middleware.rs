use axum::{
    extract::{Request, State},
    http::HeaderMap,
    middleware::Next,
    response::Response,
};
use sha2::{Digest, Sha256};

use crate::{error::AppError, state::AppState};

const ADMIN_SECRET_HEADER: &str = "x-admin-secret";

/// Middleware yang hanya meloloskan request dengan header `X-Admin-Secret` yang sama dengan
/// `ADMIN_SECRET`.
///
/// Header yang tidak ada, salah, atau `ADMIN_SECRET` yang tidak di-set dijawab `403`. Yang
/// dibandingkan adalah hash SHA-256 keduanya, bukan teks mentahnya, agar lama perbandingan tidak
/// membocorkan berapa banyak karakter awal secret yang sudah benar (timing attack).
pub async fn require_admin_secret(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let expected = state.admin_secret.as_deref().ok_or(AppError::Forbidden)?;
    let sent = headers
        .get(ADMIN_SECRET_HEADER)
        .ok_or(AppError::Forbidden)?
        .as_bytes();

    // Comparing hashes keeps the time taken from revealing how much of the secret matched.
    if Sha256::digest(sent) != Sha256::digest(expected.as_bytes()) {
        return Err(AppError::Forbidden);
    }

    Ok(next.run(request).await)
}
