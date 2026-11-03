use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[derive(Debug)]
pub enum AppError {
    Unauthorized,
    Forbidden,
    NotFound,
    Internal,
    ServiceUnavailable,
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        eprintln!("database error: {err}");
        AppError::Internal
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::Unauthorized => StatusCode::UNAUTHORIZED.into_response(),
            AppError::Forbidden => StatusCode::FORBIDDEN.into_response(),
            AppError::NotFound => StatusCode::NOT_FOUND.into_response(),
            AppError::Internal => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            AppError::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        }
    }
}
