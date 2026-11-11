use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, Response, StatusCode, header},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

use crate::{app, config::Config, features::auth::User, state::AppState};

const URI: &str = "/api/v1/templat-sp-dpa";

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
        upload_dir: Default::default(),
        surat_signing_key: None,
        smtp: None,
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

async fn cookie_with_role(db: &PgPool, state: &AppState, email: &str, table: &str) -> String {
    let user = insert_pengguna(db, email).await;
    let sql = match table {
        "mahasiswa" => "INSERT INTO mahasiswa (id_pengguna) VALUES ($1)",
        _ => "INSERT INTO panitia_magang (id_pengguna) VALUES ($1)",
    };
    sqlx::query(sql)
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

async fn put(app: &Router, cookie: Option<&str>, body: Value) -> Response<Body> {
    let mut request = Request::builder()
        .method("PUT")
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

async fn row_count(db: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM templat_sp_dpa")
        .fetch_one(db)
        .await
        .unwrap()
}

#[sqlx::test]
async fn is_null_until_panitia_magang_sets_a_link(db: PgPool) {
    let (app, state) = setup(db.clone());
    let panitia_magang = cookie_with_role(&db, &state, "panitia@gmail.com", "panitia_magang").await;
    let mahasiswa = cookie_with_role(&db, &state, "m@student.ub.ac.id", "mahasiswa").await;

    for cookie in [&panitia_magang, &mahasiswa] {
        let response = get(&app, Some(cookie)).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(json_body(response).await, Value::Null);
    }
}

#[sqlx::test]
async fn panitia_magang_sets_the_link_and_everyone_logged_in_reads_the_latest(db: PgPool) {
    let (app, state) = setup(db.clone());
    let panitia_magang = cookie_with_role(&db, &state, "panitia@gmail.com", "panitia_magang").await;
    let mahasiswa = cookie_with_role(&db, &state, "m@student.ub.ac.id", "mahasiswa").await;

    let response = put(
        &app,
        Some(&panitia_magang),
        json!({ "tautan_templat": "  https://drive.google.com/file/d/lama  " }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let lama = json_body(response).await;
    assert_eq!(
        lama["tautan_templat"],
        "https://drive.google.com/file/d/lama"
    );
    assert!(lama["updated_at"].as_str().unwrap().ends_with("+07:00"));
    assert_eq!(lama.as_object().unwrap().len(), 3);

    let baru = json_body(
        put(
            &app,
            Some(&panitia_magang),
            json!({ "tautan_templat": "http://contoh.ac.id/templat.docx" }),
        )
        .await,
    )
    .await;
    assert_ne!(baru["id_templat"], lama["id_templat"]);

    for cookie in [&panitia_magang, &mahasiswa] {
        assert_eq!(json_body(get(&app, Some(cookie)).await).await, baru);
    }
    // The old link is kept as history.
    assert_eq!(row_count(&db).await, 2);
}

#[sqlx::test]
async fn rejects_links_that_are_not_full_http_urls(db: PgPool) {
    let (app, state) = setup(db.clone());
    let panitia_magang = cookie_with_role(&db, &state, "panitia@gmail.com", "panitia_magang").await;

    let terlalu_panjang = format!("https://contoh.ac.id/{}", "x".repeat(240));
    for body in [
        json!({ "tautan_templat": "" }),
        json!({ "tautan_templat": "   " }),
        json!({ "tautan_templat": "drive.google.com/file" }),
        json!({ "tautan_templat": "ftp://contoh.ac.id/templat" }),
        json!({ "tautan_templat": "javascript:alert(1)" }),
        json!({ "tautan_templat": "https://" }),
        json!({ "tautan_templat": "https://contoh.ac.id/ada spasi" }),
        json!({ "tautan_templat": terlalu_panjang }),
        json!({ "tautan_templat": 1 }),
        json!({}),
        json!({ "tautan_templat": "https://contoh.ac.id", "lain": 1 }),
    ] {
        let response = put(&app, Some(&panitia_magang), body.clone()).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}"
        );
    }
    assert_eq!(row_count(&db).await, 0);
}

#[sqlx::test]
async fn only_panitia_magang_can_set_the_link_and_reading_needs_a_role(db: PgPool) {
    let (app, state) = setup(db.clone());
    let mahasiswa = cookie_with_role(&db, &state, "m@student.ub.ac.id", "mahasiswa").await;
    let tanpa_peran = cookie_for(&state, &insert_pengguna(&db, "x@gmail.com").await);
    let body = json!({ "tautan_templat": "https://contoh.ac.id/templat" });

    for (cookie, expected) in [
        (None, StatusCode::UNAUTHORIZED),
        (Some(mahasiswa.as_str()), StatusCode::FORBIDDEN),
        (Some(tanpa_peran.as_str()), StatusCode::FORBIDDEN),
    ] {
        let response = put(&app, cookie, body.clone()).await;
        assert_eq!(response.status(), expected);
    }
    assert_eq!(row_count(&db).await, 0);

    assert_eq!(get(&app, None).await.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        get(&app, Some(&tanpa_peran)).await.status(),
        StatusCode::FORBIDDEN
    );
}
