use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};

use super::repository;
use crate::{error::AppError, features::auth::User, state::AppState};

/// Lets only pengguna with a mahasiswa row through, and hands that row to the handler.
///
/// The `User` extractor answers 401 for a missing or invalid access token, which is what tells
/// the frontend to refresh. A valid token without a mahasiswa row is answered with 403.
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
