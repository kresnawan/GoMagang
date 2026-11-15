use std::time::Duration;

use axum::{Json, extract::State};
use serde::Serialize;

use crate::{error::AppError, state::AppState};

/// Batas waktu database menjawab. Tanpa batas ini, database yang tidak terjangkau akan menahan
/// pemeriksaan selama acquire timeout pool (30 detik), lebih lama dari batas tunggu kebanyakan
/// monitor.
const DB_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Serialize)]
pub struct Health {
    status: &'static str,
}

/// `GET /health`: memeriksa apakah server hidup dan bisa menjangkau database.
///
/// Menjalankan `SELECT 1` ke database dengan batas waktu [`DB_TIMEOUT`]. Bila berhasil, dijawab
/// `200` dengan `{"status":"ok"}`. Bila database gagal menjawab atau terlalu lambat, dijawab `503`
/// tanpa body, dan penyebabnya dicatat ke log.
pub async fn health(State(state): State<AppState>) -> Result<Json<Health>, AppError> {
    match tokio::time::timeout(DB_TIMEOUT, sqlx::query("SELECT 1").execute(&state.db)).await {
        Ok(Ok(_)) => Ok(Json(Health { status: "ok" })),
        Ok(Err(err)) => {
            tracing::error!(error = %err, "health check: database error");
            Err(AppError::ServiceUnavailable)
        }
        Err(_) => {
            tracing::error!(
                timeout_ms = DB_TIMEOUT.as_millis() as u64,
                "health check: database did not answer in time"
            );
            Err(AppError::ServiceUnavailable)
        }
    }
}
