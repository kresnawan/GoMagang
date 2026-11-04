use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, Response, StatusCode, header},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

use crate::{
    app,
    config::Config,
    features::auth::{Peran, find_peran},
    state::AppState,
};

const URI: &str = "/api/v1/tim-mb";
const SECRET: &str = "admin-secret-with-at-least-32-bytes!";

fn config(admin_secret: Option<&str>) -> Config {
    Config {
        database_url: String::new(),
        cors_allowed_origins: vec!["http://frontend.test".parse().unwrap()],
        google_client_id: "client".to_string(),
        google_client_secret: "secret".to_string(),
        google_redirect_uri: "http://backend.test/auth/google".to_string(),
        frontend_url: "http://frontend.test".to_string(),
        jwt_secret: "test-secret-with-at-least-32-bytes!".to_string(),
        admin_secret: admin_secret.map(str::to_string),
        cookie_secure: false,
    }
}

fn app_with(db: PgPool, admin_secret: Option<&str>) -> Router {
    let config = config(admin_secret);
    app::build(&config, AppState::new(&config, db))
}

async fn post(app: Router, secret: Option<&str>, body: Value) -> Response<Body> {
    let mut request = Request::builder()
        .method("POST")
        .uri(URI)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(secret) = secret {
        request = request.header("x-admin-secret", secret);
    }
    app.oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

async fn json_body(response: Response<Body>) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn pengguna_count(db: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM pengguna")
        .fetch_one(db)
        .await
        .unwrap()
}

#[sqlx::test]
async fn registers_a_tim_mb_that_can_bind_google_at_first_login(db: PgPool) {
    let app = app_with(db.clone(), Some(SECRET));

    let response = post(
        app,
        Some(SECRET),
        json!({ "email": "  Tim.MB@Gmail.com ", "nama": " Budi ", "no_hp": "081234567890" }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    let id = body["id_pengguna"].as_i64().unwrap() as i32;
    assert_eq!(
        body,
        json!({
            "id_pengguna": id,
            "email": "tim.mb@gmail.com",
            "nama": "Budi",
            "no_hp": "081234567890",
        })
    );

    let google_sub: Option<String> =
        sqlx::query_scalar("SELECT google_sub FROM pengguna WHERE id_pengguna = $1")
            .bind(id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(google_sub, None);
    assert_eq!(find_peran(&db, id).await.unwrap(), Some(Peran::TimMb));
}

#[sqlx::test]
async fn nama_and_no_hp_are_optional(db: PgPool) {
    let app = app_with(db, Some(SECRET));

    let response = post(app, Some(SECRET), json!({ "email": "tim@gmail.com" })).await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    assert_eq!(body["nama"], Value::Null);
    assert_eq!(body["no_hp"], Value::Null);
}

#[sqlx::test]
async fn the_route_does_not_exist_without_a_configured_secret(db: PgPool) {
    let app = app_with(db.clone(), None);

    let response = post(app, Some(SECRET), json!({ "email": "tim@gmail.com" })).await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(pengguna_count(&db).await, 0);
}

#[sqlx::test]
async fn rejects_a_missing_or_wrong_secret_before_reading_the_body(db: PgPool) {
    let app = app_with(db.clone(), Some(SECRET));

    let cases = [
        (None, json!({ "email": "tim@gmail.com" })),
        (Some("wrong"), json!({ "email": "tim@gmail.com" })),
        (Some(""), json!({ "email": "tim@gmail.com" })),
        (None, json!({ "invalid": "body" })),
    ];

    for (secret, body) in cases {
        let response = post(app.clone(), secret, body).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{secret:?}");
    }
    assert_eq!(pengguna_count(&db).await, 0);
}

#[sqlx::test]
async fn rejects_invalid_values_without_creating_anything(db: PgPool) {
    let app = app_with(db.clone(), Some(SECRET));

    let cases = [
        json!({ "email": "tim@student.ub.ac.id" }),
        json!({ "email": "tim@yahoo.com" }),
        json!({ "email": "tim@evilgmail.com" }),
        json!({ "email": "@gmail.com" }),
        json!({ "email": "tim" }),
        json!({ "email": "a b@gmail.com" }),
        json!({ "email": "a@b@gmail.com" }),
        json!({ "email": "tim@gmail.com", "nama": "   " }),
        json!({ "email": "tim@gmail.com", "no_hp": "0812" }),
        json!({ "email": "tim@gmail.com", "no_hp": "0812-3456-7890" }),
    ];

    for body in cases {
        let response = post(app.clone(), Some(SECRET), body.clone()).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}"
        );
        assert!(json_body(response).await["message"].is_string(), "{body}");
    }

    for body in [
        json!({}),
        json!({ "email": null }),
        json!({ "email": "tim@gmail.com", "id_pengguna": 1 }),
    ] {
        let response = post(app.clone(), Some(SECRET), body.clone()).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}"
        );
    }

    assert_eq!(pengguna_count(&db).await, 0);
}

#[sqlx::test]
async fn rejects_an_email_that_is_already_taken(db: PgPool) {
    let app = app_with(db.clone(), Some(SECRET));
    sqlx::query("INSERT INTO pengguna (email) VALUES ('Lama@gmail.com')")
        .execute(&db)
        .await
        .unwrap();

    let first = post(
        app.clone(),
        Some(SECRET),
        json!({ "email": "tim@gmail.com" }),
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);

    for email in ["tim@gmail.com", "TIM@gmail.com", "lama@gmail.com"] {
        let response = post(app.clone(), Some(SECRET), json!({ "email": email })).await;
        assert_eq!(response.status(), StatusCode::CONFLICT, "{email}");
        assert!(json_body(response).await["message"].is_string(), "{email}");
    }
    assert_eq!(pengguna_count(&db).await, 2);
}

#[sqlx::test]
async fn browsers_are_not_allowed_to_send_the_secret_header(db: PgPool) {
    let app = app_with(db, Some(SECRET));

    let response = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri(URI)
                .header(header::ORIGIN, "http://frontend.test")
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "x-admin-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let allowed = response.headers()[header::ACCESS_CONTROL_ALLOW_HEADERS]
        .to_str()
        .unwrap();
    assert!(!allowed.contains("x-admin-secret"), "{allowed}");
}
