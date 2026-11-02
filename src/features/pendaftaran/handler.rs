use axum::extract::Path;

pub async fn list() -> &'static str {
    "TODO: list pendaftaran"
}

pub async fn detail(Path(id): Path<String>) -> String {
    format!("TODO: detail pendaftaran {id}")
}
