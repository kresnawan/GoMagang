use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, Response, StatusCode, header},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

use crate::{app, config::Config, features::auth::User, state::AppState};

const URI: &str = "/api/v1/perusahaan";

fn setup(db: PgPool) -> (Router, AppState) {
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
    };
    let state = AppState::new(&config, db);
    (app::build(&config, state.clone()), state)
}

async fn insert_pengguna(db: &PgPool, email: &str) -> User {
    let id_pengguna = sqlx::query_scalar(
        "INSERT INTO pengguna (email, google_sub) VALUES ($1, $1) RETURNING id_pengguna",
    )
    .bind(email)
    .fetch_one(db)
    .await
    .unwrap();

    User {
        id_pengguna,
        email: email.to_string(),
    }
}

fn cookie_for(state: &AppState, user: &User) -> String {
    let cookie = state.sessions.access_cookie(user).unwrap();
    format!("{}={}", cookie.name(), cookie.value())
}

async fn mahasiswa_cookie(db: &PgPool, state: &AppState) -> String {
    let user = insert_pengguna(db, "m@student.ub.ac.id").await;
    sqlx::query("INSERT INTO mahasiswa (id_pengguna) VALUES ($1)")
        .bind(user.id_pengguna)
        .execute(db)
        .await
        .unwrap();
    cookie_for(state, &user)
}

async fn tim_mb_cookie(db: &PgPool, state: &AppState) -> String {
    let user = insert_pengguna(db, "tim@gmail.com").await;
    sqlx::query("INSERT INTO tim_mb (id_pengguna) VALUES ($1)")
        .bind(user.id_pengguna)
        .execute(db)
        .await
        .unwrap();
    cookie_for(state, &user)
}

async fn get(app: &Router, cookie: Option<&str>) -> Response<Body> {
    let mut request = Request::builder().uri(URI);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    app.clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn post(app: &Router, cookie: Option<&str>, body: Value) -> Response<Body> {
    let mut request = Request::builder()
        .method("POST")
        .uri(URI)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    app.clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

async fn json_body(response: Response<Body>) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn perusahaan_count(db: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM perusahaan_mitra")
        .fetch_one(db)
        .await
        .unwrap()
}

#[sqlx::test]
async fn tim_mb_creates_a_company_and_everyone_logged_in_can_list_it(db: PgPool) {
    let (app, state) = setup(db.clone());
    let tim_mb = tim_mb_cookie(&db, &state).await;
    let mahasiswa = mahasiswa_cookie(&db, &state).await;

    let response = post(
        &app,
        Some(&tim_mb),
        json!({ "nama_perusahaan": "  PT Satu  " }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let created = json_body(response).await;
    assert!(created["id_perusahaan"].is_i64());
    assert_eq!(created["nama_perusahaan"], "PT Satu");
    assert_eq!(created.as_object().unwrap().len(), 2);

    for cookie in [&tim_mb, &mahasiswa] {
        let response = get(&app, Some(cookie)).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(json_body(response).await, json!([created]));
    }
}

#[sqlx::test]
async fn lists_companies_by_name_and_empty_when_there_are_none(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = tim_mb_cookie(&db, &state).await;

    let response = get(&app, Some(&cookie)).await;
    assert_eq!(json_body(response).await, json!([]));

    for nama in ["PT Dua", "CV Tiga", "PT Satu"] {
        post(&app, Some(&cookie), json!({ "nama_perusahaan": nama })).await;
    }

    let list = json_body(get(&app, Some(&cookie)).await).await;
    let names: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["nama_perusahaan"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["CV Tiga", "PT Dua", "PT Satu"]);
}

#[sqlx::test]
async fn rejects_a_name_that_is_already_taken(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = tim_mb_cookie(&db, &state).await;
    post(&app, Some(&cookie), json!({ "nama_perusahaan": "PT Satu" })).await;

    for nama in ["PT Satu", "pt satu", "PT SATU", "  PT Satu  "] {
        let response = post(&app, Some(&cookie), json!({ "nama_perusahaan": nama })).await;
        assert_eq!(response.status(), StatusCode::CONFLICT, "{nama}");
        assert!(json_body(response).await["message"].is_string(), "{nama}");
    }
    assert_eq!(perusahaan_count(&db).await, 1);
}

#[sqlx::test]
async fn only_one_of_several_concurrent_requests_with_the_same_name_wins(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = tim_mb_cookie(&db, &state).await;
    let body = json!({ "nama_perusahaan": "PT Satu" });

    let (a, b, c, d) = tokio::join!(
        post(&app, Some(&cookie), body.clone()),
        post(&app, Some(&cookie), body.clone()),
        post(&app, Some(&cookie), body.clone()),
        post(&app, Some(&cookie), body.clone()),
    );

    let created = [a, b, c, d]
        .iter()
        .filter(|response| response.status() == StatusCode::CREATED)
        .count();
    assert_eq!(created, 1);
    assert_eq!(perusahaan_count(&db).await, 1);
}

#[sqlx::test]
async fn rejects_invalid_values_without_saving(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = tim_mb_cookie(&db, &state).await;

    for body in [
        json!({ "nama_perusahaan": "   " }),
        json!({ "nama_perusahaan": "x".repeat(256) }),
    ] {
        let response = post(&app, Some(&cookie), body.clone()).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}"
        );
        assert!(json_body(response).await["message"].is_string(), "{body}");
    }

    for body in [
        json!({}),
        json!({ "nama_perusahaan": null }),
        json!({ "nama_perusahaan": 1 }),
        json!({ "nama_perusahaan": "PT Satu", "id_perusahaan": 1 }),
    ] {
        let response = post(&app, Some(&cookie), body.clone()).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}"
        );
    }

    assert_eq!(perusahaan_count(&db).await, 0);
}

#[sqlx::test]
async fn only_tim_mb_can_create_companies(db: PgPool) {
    let (app, state) = setup(db.clone());
    let body = json!({ "nama_perusahaan": "PT Satu" });

    let without_token = post(&app, None, body.clone()).await;
    assert_eq!(without_token.status(), StatusCode::UNAUTHORIZED);
    let bad_token = post(&app, Some("access_token=not-a-jwt"), body.clone()).await;
    assert_eq!(bad_token.status(), StatusCode::UNAUTHORIZED);

    let mahasiswa = mahasiswa_cookie(&db, &state).await;
    let response = post(&app, Some(&mahasiswa), body.clone()).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let user = insert_pengguna(&db, "tanpa.peran@gmail.com").await;
    let no_role = cookie_for(&state, &user);
    let response = post(&app, Some(&no_role), body).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    assert_eq!(perusahaan_count(&db).await, 0);
}

#[sqlx::test]
async fn listing_needs_a_token_and_a_known_role(db: PgPool) {
    let (app, state) = setup(db.clone());

    assert_eq!(get(&app, None).await.status(), StatusCode::UNAUTHORIZED);

    let user = insert_pengguna(&db, "tanpa.peran@gmail.com").await;
    let cookie = cookie_for(&state, &user);
    assert_eq!(
        get(&app, Some(&cookie)).await.status(),
        StatusCode::FORBIDDEN
    );
}
