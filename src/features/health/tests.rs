use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

use crate::{app, config::Config, state::AppState};

fn setup(db: PgPool) -> Router {
    let config = Config {
        database_url: String::new(),
        cors_allowed_origins: vec![],
        google_client_id: "client".to_string(),
        google_client_secret: "secret".to_string(),
        google_redirect_uri: "http://backend.test/auth/google".to_string(),
        frontend_url: "http://frontend.test".to_string(),
        jwt_secret: "test-secret-with-at-least-32-bytes!".to_string(),
        admin_secret: None,
        cookie_secure: false,
        upload_dir: Default::default(),
        surat_signing_key: None,
        smtp: None,
    };
    let state = AppState::new(&config, db);
    app::build(&config, state)
}

async fn get_health(app: Router) -> (StatusCode, Vec<u8>) {
    let response = app
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, body.to_vec())
}

#[sqlx::test]
async fn health_ok_when_database_reachable(db: PgPool) {
    let (status, body) = get_health(setup(db)).await;

    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body, json!({ "status": "ok" }));
}

#[sqlx::test]
async fn health_needs_no_login(db: PgPool) {
    // No cookie at all, and still not 401.
    let (status, _) = get_health(setup(db)).await;

    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test]
async fn health_unavailable_when_database_unreachable(db: PgPool) {
    let app = setup(db.clone());
    db.close().await;

    let (status, body) = get_health(app).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body.is_empty());
}

#[sqlx::test]
async fn health_rejects_other_methods(db: PgPool) {
    let response = setup(db)
        .oneshot(Request::post("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}
