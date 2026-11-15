use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug)]
pub enum AppError {
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict(&'static str),
    Validation(&'static str),
    Internal,
    ServiceUnavailable,
}

#[derive(Serialize)]
struct ErrorBody {
    message: &'static str,
}

impl From<sqlx::Error> for AppError {
    /// Mengubah error database menjadi [`AppError::Internal`].
    ///
    /// Detail error hanya dicetak ke log server dan tidak pernah dikirim ke klien, supaya struktur
    /// database dan isi query tidak bocor. Dengan implementasi ini, handler cukup memakai operator
    /// `?`
    /// pada hasil query.
    fn from(err: sqlx::Error) -> Self {
        tracing::error!(error = %err, "database error");
        AppError::Internal
    }
}

impl IntoResponse for AppError {
    /// Mengubah [`AppError`] menjadi respons HTTP.
    ///
    /// - `Unauthorized` → `401`, `Forbidden` → `403`, `NotFound` → `404`, `Internal` → `500`, dan
    ///   `ServiceUnavailable` → `503`, semuanya tanpa body.
    /// - `Conflict` → `409` dan `Validation` → `422`, dengan body JSON `{"message": "..."}` yang
    ///   berisi
    ///   pesan berbahasa Indonesia dan bisa langsung ditampilkan oleh frontend.
    fn into_response(self) -> Response {
        match self {
            AppError::Unauthorized => StatusCode::UNAUTHORIZED.into_response(),
            AppError::Forbidden => StatusCode::FORBIDDEN.into_response(),
            AppError::NotFound => StatusCode::NOT_FOUND.into_response(),
            AppError::Conflict(message) => {
                (StatusCode::CONFLICT, Json(ErrorBody { message })).into_response()
            }
            AppError::Validation(message) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ErrorBody { message }),
            )
                .into_response(),
            AppError::Internal => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            AppError::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        }
    }
}
