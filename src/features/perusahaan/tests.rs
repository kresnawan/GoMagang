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
        upload_dir: Default::default(),
        surat_signing_key: None,
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
    assert!(created["deleted_at"].is_null());
    assert_eq!(created.as_object().unwrap().len(), 3);

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

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    cookie: Option<&str>,
    body: Option<Value>,
) -> Response<Body> {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    let body = match body {
        Some(body) => {
            request = request.header(header::CONTENT_TYPE, "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    app.clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap()
}

async fn rename(app: &Router, cookie: Option<&str>, id: i64, nama: &str) -> Response<Body> {
    let uri = format!("{URI}/{id}");
    let body = json!({ "nama_perusahaan": nama });
    send(app, "PATCH", &uri, cookie, Some(body)).await
}

async fn hide(app: &Router, cookie: Option<&str>, id: i64) -> Response<Body> {
    send(app, "DELETE", &format!("{URI}/{id}"), cookie, None).await
}

async fn list(app: &Router, cookie: &str, query: &str) -> Value {
    let response = send(app, "GET", &format!("{URI}{query}"), Some(cookie), None).await;
    assert_eq!(response.status(), StatusCode::OK, "{query}");
    json_body(response).await
}

fn names(list: &Value) -> Vec<&str> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|item| item["nama_perusahaan"].as_str().unwrap())
        .collect()
}

async fn create_ok(app: &Router, cookie: &str, nama: &str) -> i64 {
    let response = post(app, Some(cookie), json!({ "nama_perusahaan": nama })).await;
    assert_eq!(response.status(), StatusCode::CREATED, "{nama}");
    json_body(response).await["id_perusahaan"].as_i64().unwrap()
}

#[sqlx::test]
async fn tim_mb_renames_a_company(db: PgPool) {
    let (app, state) = setup(db.clone());
    let tim_mb = tim_mb_cookie(&db, &state).await;
    let mahasiswa = mahasiswa_cookie(&db, &state).await;
    let id = create_ok(&app, &tim_mb, "PT Lama").await;

    let response = rename(&app, Some(&tim_mb), id, "  PT Baru  ").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await,
        json!({ "id_perusahaan": id, "nama_perusahaan": "PT Baru", "deleted_at": null })
    );

    for cookie in [&tim_mb, &mahasiswa] {
        assert_eq!(names(&list(&app, cookie, "").await), ["PT Baru"]);
    }
    assert_eq!(perusahaan_count(&db).await, 1);
}

#[sqlx::test]
async fn renaming_keeps_names_unique_among_companies_that_are_not_hidden(db: PgPool) {
    let (app, state) = setup(db.clone());
    let tim_mb = tim_mb_cookie(&db, &state).await;
    let satu = create_ok(&app, &tim_mb, "PT Satu").await;
    let dua = create_ok(&app, &tim_mb, "PT Dua").await;
    let tersembunyi = create_ok(&app, &tim_mb, "PT Tiga").await;
    assert_eq!(
        hide(&app, Some(&tim_mb), tersembunyi).await.status(),
        StatusCode::OK
    );

    for nama in ["PT Dua", "pt dua", " PT DUA "] {
        let response = rename(&app, Some(&tim_mb), satu, nama).await;
        assert_eq!(response.status(), StatusCode::CONFLICT, "{nama}");
        assert_eq!(
            json_body(response).await["message"],
            "Perusahaan dengan nama ini sudah ada."
        );
    }

    // Its own name in another case, and the name of a hidden company, are both free.
    for nama in ["PT SATU", "PT Tiga"] {
        let response = rename(&app, Some(&tim_mb), satu, nama).await;
        assert_eq!(response.status(), StatusCode::OK, "{nama}");
    }
    assert_eq!(names(&list(&app, &tim_mb, "").await), ["PT Dua", "PT Tiga"]);
    let _ = dua;
}

#[sqlx::test]
async fn renaming_rejects_bad_names_and_unknown_companies(db: PgPool) {
    let (app, state) = setup(db.clone());
    let tim_mb = tim_mb_cookie(&db, &state).await;
    let id = create_ok(&app, &tim_mb, "PT Satu").await;

    for nama in ["", "   ", &"x".repeat(256)] {
        let response = rename(&app, Some(&tim_mb), id, nama).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
    let uri = format!("{URI}/{id}");
    let response = send(
        &app,
        "PATCH",
        &uri,
        Some(&tim_mb),
        Some(json!({ "nama": "x" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let response = send(
        &app,
        "PATCH",
        &format!("{URI}/abc"),
        Some(&tim_mb),
        Some(json!({ "nama_perusahaan": "x" })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = rename(&app, Some(&tim_mb), 999_999, "PT Baru").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    assert_eq!(names(&list(&app, &tim_mb, "").await), ["PT Satu"]);
}

#[sqlx::test]
async fn hiding_a_company_takes_it_out_of_the_list_but_keeps_its_row(db: PgPool) {
    let (app, state) = setup(db.clone());
    let tim_mb = tim_mb_cookie(&db, &state).await;
    let mahasiswa = mahasiswa_cookie(&db, &state).await;
    let tetap = create_ok(&app, &tim_mb, "PT Tetap").await;
    let hilang = create_ok(&app, &tim_mb, "PT Hilang").await;

    let response = hide(&app, Some(&tim_mb), hilang).await;
    assert_eq!(response.status(), StatusCode::OK);
    let hidden = json_body(response).await;
    assert_eq!(hidden["id_perusahaan"], hilang);
    assert_eq!(hidden["nama_perusahaan"], "PT Hilang");
    let deleted_at = hidden["deleted_at"].as_str().unwrap();
    assert!(deleted_at.ends_with("+07:00"), "{deleted_at}");

    // The time is WIB, whatever the database server's own time zone.
    let is_wib: bool = sqlx::query_scalar(
        "SELECT deleted_at BETWEEN (now() AT TIME ZONE 'Asia/Jakarta') - interval '1 minute'
                            AND (now() AT TIME ZONE 'Asia/Jakarta') + interval '1 minute'
         FROM perusahaan_mitra WHERE id_perusahaan = $1",
    )
    .bind(hilang as i32)
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(is_wib);

    for cookie in [&tim_mb, &mahasiswa] {
        assert_eq!(names(&list(&app, cookie, "").await), ["PT Tetap"]);
    }
    assert_eq!(perusahaan_count(&db).await, 2);
    let _ = tetap;
}

#[sqlx::test]
async fn tim_mb_can_list_the_hidden_companies_after_the_others(db: PgPool) {
    let (app, state) = setup(db.clone());
    let tim_mb = tim_mb_cookie(&db, &state).await;
    let mahasiswa = mahasiswa_cookie(&db, &state).await;
    let a = create_ok(&app, &tim_mb, "PT Awal").await;
    create_ok(&app, &tim_mb, "PT Zebra").await;
    create_ok(&app, &tim_mb, "PT Biasa").await;
    assert_eq!(hide(&app, Some(&tim_mb), a).await.status(), StatusCode::OK);

    let semua = list(&app, &tim_mb, "?termasuk_dihapus=true").await;
    assert_eq!(names(&semua), ["PT Biasa", "PT Zebra", "PT Awal"]);
    assert!(semua[0]["deleted_at"].is_null());
    assert!(semua[2]["deleted_at"].is_string());

    assert_eq!(
        names(&list(&app, &tim_mb, "?termasuk_dihapus=false").await),
        ["PT Biasa", "PT Zebra"]
    );

    // Only asking for the hidden ones is Tim MB's alone.
    let uri = format!("{URI}?termasuk_dihapus=true");
    let response = send(&app, "GET", &uri, Some(&mahasiswa), None).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        names(&list(&app, &mahasiswa, "?termasuk_dihapus=false").await),
        ["PT Biasa", "PT Zebra"]
    );
    for query in ["?termasuk_dihapus=ya", "?dihapus=true"] {
        let uri = format!("{URI}{query}");
        let response = send(&app, "GET", &uri, Some(&tim_mb), None).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{query}");
    }
}

#[sqlx::test]
async fn a_hidden_company_cannot_be_hidden_or_renamed_again_but_its_name_is_free(db: PgPool) {
    let (app, state) = setup(db.clone());
    let tim_mb = tim_mb_cookie(&db, &state).await;
    let id = create_ok(&app, &tim_mb, "PT Satu").await;
    assert_eq!(hide(&app, Some(&tim_mb), id).await.status(), StatusCode::OK);

    assert_eq!(
        hide(&app, Some(&tim_mb), id).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        hide(&app, Some(&tim_mb), 999_999).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        rename(&app, Some(&tim_mb), id, "PT Lain").await.status(),
        StatusCode::NOT_FOUND
    );
    let response = send(&app, "DELETE", &format!("{URI}/abc"), Some(&tim_mb), None).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let baru = create_ok(&app, &tim_mb, "pt satu").await;
    assert_ne!(baru, id);
    let semua = list(&app, &tim_mb, "?termasuk_dihapus=true").await;
    assert_eq!(names(&semua), ["pt satu", "PT Satu"]);
}

#[sqlx::test]
async fn only_tim_mb_can_rename_or_hide(db: PgPool) {
    let (app, state) = setup(db.clone());
    let tim_mb = tim_mb_cookie(&db, &state).await;
    let mahasiswa = mahasiswa_cookie(&db, &state).await;
    let user = insert_pengguna(&db, "tanpa.peran@gmail.com").await;
    let no_role = cookie_for(&state, &user);
    let id = create_ok(&app, &tim_mb, "PT Satu").await;

    assert_eq!(
        rename(&app, None, id, "PT Baru").await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        hide(&app, None, id).await.status(),
        StatusCode::UNAUTHORIZED
    );
    for cookie in [&mahasiswa, &no_role] {
        assert_eq!(
            rename(&app, Some(cookie), id, "PT Baru").await.status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            hide(&app, Some(cookie), id).await.status(),
            StatusCode::FORBIDDEN
        );
    }

    assert_eq!(
        names(&list(&app, &tim_mb, "?termasuk_dihapus=true").await),
        ["PT Satu"]
    );
}

#[sqlx::test]
async fn a_rename_and_a_create_cannot_both_take_one_name(db: PgPool) {
    let (app, state) = setup(db.clone());
    let tim_mb = tim_mb_cookie(&db, &state).await;
    let id = create_ok(&app, &tim_mb, "PT Satu").await;

    let (renamed, created) = tokio::join!(
        rename(&app, Some(&tim_mb), id, "PT Sama"),
        post(&app, Some(&tim_mb), json!({ "nama_perusahaan": "PT Sama" })),
    );
    let mut statuses = [renamed.status(), created.status()];
    statuses.sort();
    assert!(
        statuses == [StatusCode::OK, StatusCode::CONFLICT]
            || statuses == [StatusCode::CREATED, StatusCode::CONFLICT],
        "{statuses:?}"
    );

    let sama: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM perusahaan_mitra WHERE lower(nama_perusahaan) = 'pt sama'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(sama, 1);
}
