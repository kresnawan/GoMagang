use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Form, Json, Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, Response, StatusCode, header},
    routing::post,
};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;

use super::{GoogleClient, repository, session::hash_token};
use crate::{app, config::Config, state::AppState};

const CLIENT_ID: &str = "test-client-id";
const CLIENT_SECRET: &str = "test-client-secret";
const FRONTEND_URL: &str = "http://frontend.test";
const JWT_SECRET: &str = "test-secret-with-at-least-32-bytes!";
const CALLBACK: &str = "/auth/google?code=good-code&state=abc";
const STATE_COOKIE: &str = "oauth_state=abc";

fn config() -> Config {
    Config {
        database_url: String::new(),
        cors_allowed_origins: vec![],
        google_client_id: CLIENT_ID.to_string(),
        google_client_secret: CLIENT_SECRET.to_string(),
        google_redirect_uri: "http://backend.test/auth/google".to_string(),
        frontend_url: FRONTEND_URL.to_string(),
        jwt_secret: JWT_SECRET.to_string(),
        admin_secret: None,
        cookie_secure: false,
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn sign(claims: &Value, secret: &str) -> String {
    let key = EncodingKey::from_secret(secret.as_bytes());
    encode(&Header::default(), claims, &key).unwrap()
}

fn id_token_claims() -> Value {
    json!({
        "sub": "1234567890",
        "email": "mahasiswa@student.ub.ac.id",
        "email_verified": true,
        "hd": "student.ub.ac.id",
        "aud": CLIENT_ID,
        "iss": "https://accounts.google.com",
        "exp": now() + 3600,
    })
}

struct MockGoogle {
    calls: Arc<AtomicUsize>,
    router: Router,
}

/// Serves `/token` on a local port. Only the code `good-code` is accepted.
async fn app_with_google(db: PgPool, claims: Value) -> MockGoogle {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let id_token = sign(&claims, "google-key");
    let token_endpoint = move |Form(form): Form<HashMap<String, String>>| async move {
        counter.fetch_add(1, Ordering::SeqCst);
        let valid = form["code"] == "good-code"
            && form["client_id"] == CLIENT_ID
            && form["client_secret"] == CLIENT_SECRET
            && form["grant_type"] == "authorization_code"
            && form["redirect_uri"] == "http://backend.test/auth/google";
        if valid {
            (StatusCode::OK, Json(json!({ "id_token": id_token })))
        } else {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "invalid_grant" })),
            )
        }
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let mock = Router::new().route("/token", post(token_endpoint));
        axum::serve(listener, mock).await.unwrap();
    });

    let config = config();
    let mut state = AppState::new(&config, db);
    state.google =
        Arc::new(GoogleClient::new(&config).with_token_url(format!("http://{addr}/token")));

    MockGoogle {
        calls,
        router: app::build(&config, state),
    }
}

/// For routes that never touch the database: the pool doesn't connect until it's used.
fn app_without_db(config: &Config) -> Router {
    let db = PgPoolOptions::new()
        .connect_lazy("postgres://localhost/unused")
        .unwrap();
    app::build(config, AppState::new(config, db))
}

async fn send(app: Router, method: &str, uri: &str, cookie: Option<&str>) -> Response<Body> {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    app.oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

fn location(response: &Response<Body>) -> &str {
    response.headers()[header::LOCATION].to_str().unwrap()
}

fn set_cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|value| value.to_str().unwrap())
        .find(|value| value.starts_with(&format!("{name}=")))
}

/// The `name=value` part of a `Set-Cookie` header, which is what the browser sends back.
fn cookie_pair(set_cookie: &str) -> String {
    set_cookie.split(';').next().unwrap().to_string()
}

fn max_age(set_cookie: &str) -> u64 {
    set_cookie
        .split("Max-Age=")
        .nth(1)
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn token_cookies(response: &Response<Body>) -> (String, String) {
    (
        cookie_pair(set_cookie(response.headers(), "access_token").unwrap()),
        cookie_pair(set_cookie(response.headers(), "refresh_token").unwrap()),
    )
}

fn token_hash(refresh_cookie: &str) -> Vec<u8> {
    hash_token(refresh_cookie.trim_start_matches("refresh_token="))
}

/// Logs in through the callback and returns the app with the issued cookies.
async fn login(db: PgPool) -> (Router, String, String) {
    let mock = app_with_google(db, id_token_claims()).await;
    let response = send(mock.router.clone(), "GET", CALLBACK, Some(STATE_COOKIE)).await;
    let (access, refresh) = token_cookies(&response);
    (mock.router, access, refresh)
}

async fn refresh(app: &Router, cookie: &str) -> Response<Body> {
    send(app.clone(), "POST", "/auth/refresh", Some(cookie)).await
}

async fn token_count(db: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM token_refresh")
        .fetch_one(db)
        .await
        .unwrap()
}

/// Moves the rotation of a token into the past, as if the grace period had passed.
async fn age_rotation(db: &PgPool, refresh_cookie: &str, seconds: f64) {
    sqlx::query(
        "UPDATE token_refresh SET rotated_at = now() - make_interval(secs => $2)
         WHERE hash_token = $1",
    )
    .bind(token_hash(refresh_cookie))
    .bind(seconds)
    .execute(db)
    .await
    .unwrap();
}

#[tokio::test]
async fn login_redirects_to_google_and_sets_state_cookie() {
    let response = send(app_without_db(&config()), "GET", "/auth/google/login", None).await;

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let cookie = set_cookie(response.headers(), "oauth_state").unwrap();
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax"));
    assert!(cookie.contains("Path=/auth/google"));
    assert!(!cookie.contains("Secure"));

    let pair = cookie_pair(cookie);
    let state = pair.trim_start_matches("oauth_state=");
    assert_eq!(state.len(), 64);
    let url = location(&response);
    assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
    assert!(url.contains(&format!("state={state}")));
    assert!(url.contains(&format!("client_id={CLIENT_ID}")));
    assert!(url.contains("response_type=code"));
    assert!(!url.contains("hd="), "hd would hide gmail.com accounts");
    assert!(url.contains("redirect_uri=http%3A%2F%2Fbackend.test%2Fauth%2Fgoogle"));
}

#[tokio::test]
async fn login_uses_a_new_state_each_time() {
    let app = app_without_db(&config());
    let first = send(app.clone(), "GET", "/auth/google/login", None).await;
    let second = send(app, "GET", "/auth/google/login", None).await;

    assert_ne!(
        set_cookie(first.headers(), "oauth_state"),
        set_cookie(second.headers(), "oauth_state"),
    );
}

#[sqlx::test]
async fn callback_sets_both_cookies_and_me_accepts_the_access_token(db: PgPool) {
    let mock = app_with_google(db, id_token_claims()).await;
    let response = send(mock.router.clone(), "GET", CALLBACK, Some(STATE_COOKIE)).await;

    assert_eq!(location(&response), "http://frontend.test/dasbor");
    let access = set_cookie(response.headers(), "access_token").unwrap();
    assert!(access.contains("HttpOnly"));
    assert!(access.contains("SameSite=Lax"));
    assert!(access.contains("Path=/;") || access.ends_with("Path=/"));
    assert_eq!(max_age(access), 900);
    let refresh = set_cookie(response.headers(), "refresh_token").unwrap();
    assert!(refresh.contains("HttpOnly"));
    assert!(refresh.contains("SameSite=Lax"));
    assert!(refresh.contains("Path=/auth"));
    assert_eq!(max_age(refresh), 604800);
    assert_eq!(
        max_age(set_cookie(response.headers(), "oauth_state").unwrap()),
        0
    );

    let cookie = cookie_pair(access);
    let response = send(mock.router, "GET", "/auth/me", Some(&cookie)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let user: Value = serde_json::from_slice(&body).unwrap();
    assert!(user["id_pengguna"].is_i64());
    assert_eq!(user["email"], "mahasiswa@student.ub.ac.id");
    assert!(user.get("name").is_none());
}

#[tokio::test]
async fn cookies_are_secure_when_configured() {
    let config = Config {
        cookie_secure: true,
        ..config()
    };
    let response = send(app_without_db(&config), "GET", "/auth/google/login", None).await;

    let cookie = set_cookie(response.headers(), "oauth_state").unwrap();
    assert!(cookie.contains("Secure"));
}

#[sqlx::test]
async fn refresh_tokens_are_stored_only_as_hashes(db: PgPool) {
    let (_, _, refresh_cookie) = login(db.clone()).await;
    let token = refresh_cookie.trim_start_matches("refresh_token=");

    let stored: Vec<u8> = sqlx::query_scalar("SELECT hash_token FROM token_refresh")
        .fetch_one(&db)
        .await
        .unwrap();

    assert_eq!(stored, hash_token(token));
    assert_ne!(stored, token.as_bytes());
}

#[sqlx::test]
async fn logging_in_again_updates_the_same_pengguna(db: PgPool) {
    let _ = login(db.clone()).await;
    let mut claims = id_token_claims();
    claims["email"] = json!("baru@student.ub.ac.id");
    let mock = app_with_google(db.clone(), claims).await;
    send(mock.router, "GET", CALLBACK, Some(STATE_COOKIE)).await;

    let emails: Vec<String> = sqlx::query_scalar("SELECT email FROM pengguna")
        .fetch_all(&db)
        .await
        .unwrap();
    assert_eq!(emails, ["baru@student.ub.ac.id"]);
}

async fn row_counts(db: &PgPool) -> (i64, i64) {
    let pengguna = sqlx::query_scalar("SELECT count(*) FROM pengguna")
        .fetch_one(db)
        .await
        .unwrap();
    let mahasiswa = sqlx::query_scalar("SELECT count(*) FROM mahasiswa")
        .fetch_one(db)
        .await
        .unwrap();
    (pengguna, mahasiswa)
}

#[sqlx::test]
async fn login_creates_pengguna_and_mahasiswa(db: PgPool) {
    let (app, access, _) = login(db.clone()).await;

    assert_eq!(row_counts(&db).await, (1, 1));

    let response = send(app, "GET", "/api/v1/mahasiswa/data", Some(&access)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let mahasiswa: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(mahasiswa["email"], "mahasiswa@student.ub.ac.id");
    assert_eq!(mahasiswa["nim"], Value::Null);
}

#[sqlx::test]
async fn logging_in_again_keeps_the_mahasiswa_data(db: PgPool) {
    let _ = login(db.clone()).await;
    sqlx::query("UPDATE mahasiswa SET nim = '215150200111001', nama = 'Budi'")
        .execute(&db)
        .await
        .unwrap();

    let _ = login(db.clone()).await;

    assert_eq!(row_counts(&db).await, (1, 1));
    let nama: String = sqlx::query_scalar("SELECT nama FROM mahasiswa")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(nama, "Budi");
}

#[sqlx::test]
async fn login_adds_the_mahasiswa_row_for_an_existing_pengguna(db: PgPool) {
    let _ = login(db.clone()).await;
    sqlx::query("DELETE FROM mahasiswa")
        .execute(&db)
        .await
        .unwrap();

    let _ = login(db.clone()).await;

    assert_eq!(row_counts(&db).await, (1, 1));
}

const TIM_MB_EMAIL: &str = "tim.mb@gmail.com";

fn tim_mb_claims() -> Value {
    let mut claims = id_token_claims();
    claims["sub"] = json!("tim-mb-google-sub");
    claims["email"] = json!(TIM_MB_EMAIL);
    claims.as_object_mut().unwrap().remove("hd");
    claims
}

async fn insert_pengguna_only(db: &PgPool, email: &str) -> i32 {
    sqlx::query_scalar("INSERT INTO pengguna (email) VALUES ($1) RETURNING id_pengguna")
        .bind(email)
        .fetch_one(db)
        .await
        .unwrap()
}

async fn register_tim_mb(db: &PgPool, email: &str) -> i32 {
    let id_pengguna = insert_pengguna_only(db, email).await;
    sqlx::query("INSERT INTO tim_mb (id_pengguna, nama) VALUES ($1, 'Tim MB')")
        .bind(id_pengguna)
        .execute(db)
        .await
        .unwrap();
    id_pengguna
}

async fn google_sub_of(db: &PgPool, id_pengguna: i32) -> Option<String> {
    sqlx::query_scalar("SELECT google_sub FROM pengguna WHERE id_pengguna = $1")
        .bind(id_pengguna)
        .fetch_one(db)
        .await
        .unwrap()
}

async fn login_with(db: &PgPool, claims: Value) -> Response<Body> {
    let mock = app_with_google(db.clone(), claims).await;
    send(mock.router, "GET", CALLBACK, Some(STATE_COOKIE)).await
}

#[sqlx::test]
async fn registered_tim_mb_logs_in_with_gmail_and_goes_to_their_pages(db: PgPool) {
    let id_pengguna = register_tim_mb(&db, TIM_MB_EMAIL).await;
    let mock = app_with_google(db.clone(), tim_mb_claims()).await;

    let response = send(mock.router.clone(), "GET", CALLBACK, Some(STATE_COOKIE)).await;

    assert_eq!(location(&response), "http://frontend.test/tim-mb");
    assert_eq!(
        google_sub_of(&db, id_pengguna).await.as_deref(),
        Some("tim-mb-google-sub")
    );
    assert_eq!(token_count(&db).await, 1);

    let (access, _) = token_cookies(&response);
    let me = send(mock.router.clone(), "GET", "/auth/me", Some(&access)).await;
    assert_eq!(me.status(), StatusCode::OK);

    assert_eq!(row_counts(&db).await, (1, 0), "no mahasiswa row for Tim MB");
    let data = send(mock.router, "GET", "/api/v1/mahasiswa/data", Some(&access)).await;
    assert_eq!(data.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn tim_mb_can_log_in_again_and_email_case_does_not_matter(db: PgPool) {
    register_tim_mb(&db, "Tim.MB@gmail.com").await;

    for _ in 0..2 {
        let response = login_with(&db, tim_mb_claims()).await;
        assert_eq!(location(&response), "http://frontend.test/tim-mb");
    }

    assert_eq!(row_counts(&db).await, (1, 0));
}

#[sqlx::test]
async fn another_google_account_cannot_take_over_a_tim_mb_row(db: PgPool) {
    let id_pengguna = register_tim_mb(&db, TIM_MB_EMAIL).await;
    login_with(&db, tim_mb_claims()).await;

    let mut claims = tim_mb_claims();
    claims["sub"] = json!("attacker-google-sub");
    let response = login_with(&db, claims).await;

    assert_eq!(
        location(&response),
        "http://frontend.test/login?error=not_registered"
    );
    assert!(set_cookie(response.headers(), "access_token").is_none());
    assert_eq!(
        google_sub_of(&db, id_pengguna).await.as_deref(),
        Some("tim-mb-google-sub")
    );
    assert_eq!(token_count(&db).await, 1);
}

#[sqlx::test]
async fn gmail_outside_tim_mb_is_rejected_without_creating_anything(db: PgPool) {
    let response = login_with(&db, tim_mb_claims()).await;

    assert_eq!(
        location(&response),
        "http://frontend.test/login?error=not_registered"
    );
    assert!(set_cookie(response.headers(), "access_token").is_none());
    assert_eq!(row_counts(&db).await, (0, 0));
    assert_eq!(token_count(&db).await, 0);
}

#[sqlx::test]
async fn gmail_in_pengguna_but_not_in_tim_mb_is_rejected(db: PgPool) {
    let id_pengguna = insert_pengguna_only(&db, TIM_MB_EMAIL).await;

    let response = login_with(&db, tim_mb_claims()).await;

    assert_eq!(
        location(&response),
        "http://frontend.test/login?error=not_registered"
    );
    assert_eq!(google_sub_of(&db, id_pengguna).await, None);
    assert_eq!(token_count(&db).await, 0);
}

#[sqlx::test]
async fn gmail_login_needs_a_verified_personal_account(db: PgPool) {
    register_tim_mb(&db, TIM_MB_EMAIL).await;

    let mut unverified = tim_mb_claims();
    unverified["email_verified"] = json!(false);
    let mut hosted = tim_mb_claims();
    hosted["hd"] = json!("gmail.com");
    let mut lookalike = tim_mb_claims();
    lookalike["email"] = json!("tim.mb@evilgmail.com");

    for claims in [unverified, hosted, lookalike] {
        let response = login_with(&db, claims).await;
        assert_eq!(
            location(&response),
            "http://frontend.test/login?error=domain"
        );
    }
    assert_eq!(token_count(&db).await, 0);
}

#[sqlx::test]
async fn email_owned_by_another_google_account_cannot_log_in(db: PgPool) {
    let _ = login(db.clone()).await;
    let mut claims = id_token_claims();
    claims["sub"] = json!("different-google-account");
    let mock = app_with_google(db.clone(), claims).await;

    let response = send(mock.router, "GET", CALLBACK, Some(STATE_COOKIE)).await;

    assert_eq!(
        location(&response),
        "http://frontend.test/login?error=failed"
    );
    assert!(set_cookie(response.headers(), "access_token").is_none());
    assert_eq!(token_count(&db).await, 1);
    assert_eq!(row_counts(&db).await, (1, 1));
}

#[sqlx::test]
async fn migrations_create_every_table_in_the_erd(db: PgPool) {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name::text FROM information_schema.tables
         WHERE table_schema = 'public' AND table_name <> '_sqlx_migrations'
         ORDER BY table_name",
    )
    .fetch_all(&db)
    .await
    .unwrap();

    assert_eq!(
        tables,
        [
            "berkas_pendaftaran",
            "jadwal_seleksi_substansi",
            "log_status",
            "mahasiswa",
            "mata_kuliah_konversi",
            "pendaftaran",
            "pengguna",
            "penyetaraan_mk",
            "periode_pendaftaran",
            "perusahaan_mitra",
            "plotting_dpm",
            "posisi_magang",
            "status_pendaftaran",
            "surat_keterangan",
            "templat_sp_dpa",
            "tim_mb",
            "token_refresh",
        ]
    );
}

#[sqlx::test]
async fn callback_rejects_bad_state_without_calling_google(db: PgPool) {
    let cases = [
        ("mismatch", CALLBACK, Some("oauth_state=other")),
        ("no cookie", CALLBACK, None),
        (
            "no param",
            "/auth/google?code=good-code",
            Some(STATE_COOKIE),
        ),
        ("neither", "/auth/google?code=good-code", None),
    ];

    for (name, uri, cookie) in cases {
        let mock = app_with_google(db.clone(), id_token_claims()).await;
        let response = send(mock.router, "GET", uri, cookie).await;

        assert_eq!(
            location(&response),
            "http://frontend.test/login?error=failed",
            "{name}"
        );
        assert!(
            set_cookie(response.headers(), "access_token").is_none(),
            "{name}"
        );
        assert_eq!(mock.calls.load(Ordering::SeqCst), 0, "{name}");
    }
}

#[sqlx::test]
async fn callback_reports_cancelled_login(db: PgPool) {
    let mock = app_with_google(db, id_token_claims()).await;
    let response = send(
        mock.router,
        "GET",
        "/auth/google?error=access_denied&state=abc",
        Some(STATE_COOKIE),
    )
    .await;

    assert_eq!(
        location(&response),
        "http://frontend.test/login?error=cancelled"
    );
    assert_eq!(mock.calls.load(Ordering::SeqCst), 0);
}

#[sqlx::test]
async fn callback_rejects_code_google_refuses(db: PgPool) {
    let mock = app_with_google(db, id_token_claims()).await;
    let response = send(
        mock.router,
        "GET",
        "/auth/google?code=bad-code&state=abc",
        Some(STATE_COOKIE),
    )
    .await;

    assert_eq!(
        location(&response),
        "http://frontend.test/login?error=failed"
    );
    assert!(set_cookie(response.headers(), "access_token").is_none());
}

async fn callback_location_with(db: PgPool, field: &str, value: Option<Value>) -> String {
    let mut claims = id_token_claims();
    match value {
        Some(value) => claims[field] = value,
        None => {
            claims.as_object_mut().unwrap().remove(field);
        }
    }

    let mock = app_with_google(db.clone(), claims).await;
    let response = send(mock.router, "GET", CALLBACK, Some(STATE_COOKIE)).await;

    assert!(set_cookie(response.headers(), "access_token").is_none());
    assert_eq!(token_count(&db).await, 0);
    assert_eq!(row_counts(&db).await, (0, 0));
    location(&response).to_string()
}

#[sqlx::test]
async fn callback_rejects_invalid_id_tokens(db: PgPool) {
    let cases = [
        ("wrong aud", "aud", Some(json!("other-client"))),
        ("wrong iss", "iss", Some(json!("https://evil.example"))),
        ("expired", "exp", Some(json!(now() - 1))),
        ("missing aud", "aud", None),
        ("missing iss", "iss", None),
        ("missing exp", "exp", None),
        ("missing sub", "sub", None),
        ("missing email", "email", None),
    ];

    for (name, field, value) in cases {
        assert_eq!(
            callback_location_with(db.clone(), field, value).await,
            "http://frontend.test/login?error=failed",
            "{name}"
        );
    }
}

#[sqlx::test]
async fn callback_rejects_disallowed_accounts(db: PgPool) {
    let cases = [
        ("other domain", "email", Some(json!("a@gmail.com"))),
        (
            "suffix trick",
            "email",
            Some(json!("a@evilstudent.ub.ac.id")),
        ),
        ("parent domain", "email", Some(json!("a@ub.ac.id"))),
        ("unverified", "email_verified", Some(json!(false))),
        ("missing verified", "email_verified", None),
        ("other hd", "hd", Some(json!("gmail.com"))),
        ("missing hd", "hd", None),
    ];

    for (name, field, value) in cases {
        assert_eq!(
            callback_location_with(db.clone(), field, value).await,
            "http://frontend.test/login?error=domain",
            "{name}"
        );
    }
}

fn access_claims(exp: u64) -> Value {
    json!({
        "sub": "1",
        "email": "a@student.ub.ac.id",
        "iat": 0,
        "exp": exp,
    })
}

#[tokio::test]
async fn me_rejects_missing_or_invalid_access_tokens() {
    let wrong_key = format!(
        "access_token={}",
        sign(
            &access_claims(now() + 3600),
            "another-secret-with-at-least-32-bytes"
        )
    );
    let expired = format!("access_token={}", sign(&access_claims(1000), JWT_SECRET));
    let cases = [
        ("missing", None),
        ("garbage", Some("access_token=not-a-jwt")),
        ("wrong key", Some(wrong_key.as_str())),
        ("expired", Some(expired.as_str())),
    ];

    for (name, cookie) in cases {
        let response = send(app_without_db(&config()), "GET", "/auth/me", cookie).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{name}");
    }
}

#[sqlx::test]
async fn refresh_issues_new_tokens_that_work(db: PgPool) {
    let (app, _, old_refresh) = login(db).await;

    let response = refresh(&app, &old_refresh).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let (access, new_refresh) = token_cookies(&response);
    assert_ne!(new_refresh, old_refresh);

    let me = send(app.clone(), "GET", "/auth/me", Some(&access)).await;
    assert_eq!(me.status(), StatusCode::OK);
    assert_eq!(
        refresh(&app, &new_refresh).await.status(),
        StatusCode::NO_CONTENT
    );
}

#[sqlx::test]
async fn refresh_keeps_the_absolute_expiry(db: PgPool) {
    let (app, _, old_refresh) = login(db.clone()).await;
    sqlx::query("UPDATE token_refresh SET expires_at = now() + interval '1 hour'")
        .execute(&db)
        .await
        .unwrap();

    let response = refresh(&app, &old_refresh).await;

    let cookie = set_cookie(response.headers(), "refresh_token").unwrap();
    assert!((3590..=3600).contains(&max_age(cookie)), "{cookie}");
    assert!(cookie.contains("Path=/auth"));
    let distinct_expiries: i64 =
        sqlx::query_scalar("SELECT count(DISTINCT expires_at) FROM token_refresh")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(distinct_expiries, 1);
    assert_eq!(token_count(&db).await, 2);
}

#[sqlx::test]
async fn refresh_rejects_expired_tokens_and_clears_cookies(db: PgPool) {
    let (app, access, old_refresh) = login(db.clone()).await;
    sqlx::query("UPDATE token_refresh SET expires_at = now() - interval '1 second'")
        .execute(&db)
        .await
        .unwrap();

    let cookies = format!("{access}; {old_refresh}");
    let response = refresh(&app, &cookies).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let cleared = set_cookie(response.headers(), "refresh_token").unwrap();
    assert_eq!(max_age(cleared), 0);
    assert!(cleared.contains("Path=/auth"));
    assert_eq!(
        max_age(set_cookie(response.headers(), "access_token").unwrap()),
        0
    );
}

#[sqlx::test]
async fn refresh_rejects_missing_or_unknown_tokens(db: PgPool) {
    let (app, _, _) = login(db).await;

    let missing = send(app.clone(), "POST", "/auth/refresh", None).await;
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        refresh(&app, "refresh_token=unknown").await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test]
async fn rotated_token_still_works_within_the_grace_period(db: PgPool) {
    let (app, _, old_refresh) = login(db.clone()).await;

    let first = refresh(&app, &old_refresh).await;
    let second = refresh(&app, &old_refresh).await;

    assert_eq!(first.status(), StatusCode::NO_CONTENT);
    assert_eq!(second.status(), StatusCode::NO_CONTENT);
    let (_, first_new) = token_cookies(&first);
    let (_, second_new) = token_cookies(&second);
    assert_eq!(
        refresh(&app, &first_new).await.status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        refresh(&app, &second_new).await.status(),
        StatusCode::NO_CONTENT
    );
}

#[sqlx::test]
async fn concurrent_refreshes_with_the_same_token_all_succeed(db: PgPool) {
    let (app, _, old_refresh) = login(db.clone()).await;

    let (a, b, c, d) = tokio::join!(
        refresh(&app, &old_refresh),
        refresh(&app, &old_refresh),
        refresh(&app, &old_refresh),
        refresh(&app, &old_refresh),
    );

    for response in [a, b, c, d] {
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
    assert_eq!(token_count(&db).await, 5);
}

#[sqlx::test]
async fn repeated_use_does_not_extend_the_grace_period(db: PgPool) {
    let (app, _, old_refresh) = login(db.clone()).await;
    refresh(&app, &old_refresh).await;
    age_rotation(&db, &old_refresh, 9.0).await;

    refresh(&app, &old_refresh).await;

    let rotated_long_ago: bool = sqlx::query_scalar(
        "SELECT rotated_at < now() - interval '8 seconds' FROM token_refresh WHERE hash_token = $1",
    )
    .bind(token_hash(&old_refresh))
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(rotated_long_ago);
}

#[sqlx::test]
async fn reuse_after_the_grace_period_revokes_the_family_only(db: PgPool) {
    let (app, _, stolen) = login(db.clone()).await;
    let (_, _, other_device) = login(db.clone()).await;
    let (_, current) = token_cookies(&refresh(&app, &stolen).await);
    age_rotation(&db, &stolen, 11.0).await;

    let reuse = refresh(&app, &stolen).await;
    assert_eq!(reuse.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        refresh(&app, &current).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        refresh(&app, &other_device).await.status(),
        StatusCode::NO_CONTENT
    );
}

#[sqlx::test]
async fn logout_revokes_the_family_and_clears_cookies(db: PgPool) {
    let (app, access, refresh_cookie) = login(db.clone()).await;
    let (_, _, other_device) = login(db.clone()).await;

    let cookies = format!("{access}; {refresh_cookie}");
    let response = send(app.clone(), "POST", "/auth/logout", Some(&cookies)).await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        max_age(set_cookie(response.headers(), "access_token").unwrap()),
        0
    );
    let cleared = set_cookie(response.headers(), "refresh_token").unwrap();
    assert_eq!(max_age(cleared), 0);
    assert!(cleared.contains("Path=/auth"));

    assert_eq!(
        refresh(&app, &refresh_cookie).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        refresh(&app, &other_device).await.status(),
        StatusCode::NO_CONTENT
    );
}

#[sqlx::test]
async fn logout_deletes_every_row_of_the_login_including_rotated_tokens(db: PgPool) {
    let (app, _, first) = login(db.clone()).await;
    let (_, _, other_device) = login(db.clone()).await;
    let (_, second) = token_cookies(&refresh(&app, &first).await);
    let (_, third) = token_cookies(&refresh(&app, &second).await);
    assert_eq!(token_count(&db).await, 4);

    let response = send(app, "POST", "/auth/logout", Some(&third)).await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let remaining: Vec<Vec<u8>> = sqlx::query_scalar("SELECT hash_token FROM token_refresh")
        .fetch_all(&db)
        .await
        .unwrap();
    assert_eq!(remaining, [token_hash(&other_device)]);
}

#[sqlx::test]
async fn logout_without_a_known_token_deletes_nothing_and_still_succeeds(db: PgPool) {
    let (app, _, _) = login(db.clone()).await;

    let unknown = send(
        app.clone(),
        "POST",
        "/auth/logout",
        Some("refresh_token=unknown"),
    )
    .await;
    let without_cookie = send(app, "POST", "/auth/logout", None).await;

    assert_eq!(unknown.status(), StatusCode::NO_CONTENT);
    assert_eq!(without_cookie.status(), StatusCode::NO_CONTENT);
    assert_eq!(token_count(&db).await, 1);
}

#[sqlx::test]
async fn cleanup_deletes_only_expired_tokens(db: PgPool) {
    let (_, _, expired) = login(db.clone()).await;
    let _ = login(db.clone()).await;
    sqlx::query(
        "UPDATE token_refresh SET expires_at = now() - interval '1 second' WHERE hash_token = $1",
    )
    .bind(token_hash(&expired))
    .execute(&db)
    .await
    .unwrap();

    assert_eq!(repository::delete_expired_tokens(&db).await.unwrap(), 1);
    assert_eq!(token_count(&db).await, 1);
}
