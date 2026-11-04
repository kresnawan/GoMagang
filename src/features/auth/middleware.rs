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

/// Lets only pengguna with a tim_mb row through.
///
/// The `User` extractor answers 401 for a missing or invalid access token, and any other valid
/// token is answered with 403.
pub async fn require_tim_mb(
    State(state): State<AppState>,
    user: User,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    match find_peran(&state.db, user.id_pengguna).await? {
        Some(Peran::TimMb) => Ok(next.run(request).await),
        _ => Err(AppError::Forbidden),
    }
}
