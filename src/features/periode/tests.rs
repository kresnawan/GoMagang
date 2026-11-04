use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, Response, StatusCode, header},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

use crate::{app, config::Config, features::auth::User, state::AppState};

const URI: &str = "/api/v1/periode";

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

/// `start` and `end` are intervals relative to the current time in Jakarta.
async fn insert_periode(db: &PgPool, start: &str, end: &str) -> i32 {
    sqlx::query_scalar(
        "INSERT INTO periode_pendaftaran (waktu_mulai, waktu_tutup)
         VALUES (now() AT TIME ZONE 'Asia/Jakarta' + $1::interval,
                 now() AT TIME ZONE 'Asia/Jakarta' + $2::interval)
         RETURNING id_periode",
    )
    .bind(start)
    .bind(end)
    .fetch_one(db)
    .await
    .unwrap()
}

struct Periods {
    selesai: i32,
    berlangsung: i32,
    belum_dimulai: i32,
}

async fn seed_periods(db: &PgPool) -> Periods {
    Periods {
        selesai: insert_periode(db, "-30 days", "-20 days").await,
        berlangsung: insert_periode(db, "-1 minute", "1 minute").await,
        belum_dimulai: insert_periode(db, "20 days", "30 days").await,
    }
}

async fn get(app: &Router, uri: &str, cookie: Option<&str>) -> Response<Body> {
    let mut request = Request::builder().uri(uri);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    app.clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn ids(app: &Router, uri: &str, cookie: &str) -> Vec<i32> {
    let response = get(app, uri, Some(cookie)).await;
    assert_eq!(response.status(), StatusCode::OK, "{uri}");
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let list: Vec<Value> = serde_json::from_slice(&body).unwrap();
    list.iter()
        .map(|item| item["id_periode"].as_i64().unwrap() as i32)
        .collect()
}

#[sqlx::test]
async fn returns_the_period_with_wib_times_and_its_status(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = tim_mb_cookie(&db, &state).await;
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO periode_pendaftaran (catatan, waktu_mulai, waktu_tutup)
         VALUES ('Genap', '2020-01-05 08:30:00', '2020-02-15 23:59:00')
         RETURNING id_periode",
    )
    .fetch_one(&db)
    .await
    .unwrap();

    let response = get(&app, URI, Some(&cookie)).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let list: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        list,
        json!([{
            "id_periode": id,
            "catatan": "Genap",
            "waktu_mulai": "2020-01-05T08:30:00+07:00",
            "waktu_tutup": "2020-02-15T23:59:00+07:00",
            "status": "selesai",
        }])
    );
}

#[sqlx::test]
async fn tim_mb_sees_every_period_newest_first_and_can_filter(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = tim_mb_cookie(&db, &state).await;
    let p = seed_periods(&db).await;

    let cases = [
        (
            URI.to_string(),
            vec![p.belum_dimulai, p.berlangsung, p.selesai],
        ),
        (format!("{URI}?status=belum_dimulai"), vec![p.belum_dimulai]),
        (format!("{URI}?status=berlangsung"), vec![p.berlangsung]),
        (format!("{URI}?status=selesai"), vec![p.selesai]),
        (format!("{URI}?id={}", p.selesai), vec![p.selesai]),
        (
            format!("{URI}?id={}&status=selesai", p.selesai),
            vec![p.selesai],
        ),
        (format!("{URI}?id={}&status=berlangsung", p.selesai), vec![]),
        (format!("{URI}?id=9999"), vec![]),
    ];

    for (uri, expected) in cases {
        assert_eq!(ids(&app, &uri, &cookie).await, expected, "{uri}");
    }
}

#[sqlx::test]
async fn mahasiswa_only_sees_the_running_period(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = mahasiswa_cookie(&db, &state).await;
    let p = seed_periods(&db).await;

    let cases = [
        (URI.to_string(), vec![p.berlangsung]),
        (format!("{URI}?status=berlangsung"), vec![p.berlangsung]),
        (format!("{URI}?id={}", p.berlangsung), vec![p.berlangsung]),
        (format!("{URI}?id={}", p.selesai), vec![]),
        (format!("{URI}?id={}", p.belum_dimulai), vec![]),
    ];

    for (uri, expected) in cases {
        assert_eq!(ids(&app, &uri, &cookie).await, expected, "{uri}");
    }
}

#[sqlx::test]
async fn mahasiswa_cannot_ask_for_other_statuses(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = mahasiswa_cookie(&db, &state).await;

    for status in ["belum_dimulai", "selesai"] {
        let uri = format!("{URI}?status={status}");
        let response = get(&app, &uri, Some(&cookie)).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{uri}");
    }
}

#[sqlx::test]
async fn mahasiswa_gets_nothing_when_no_period_is_running(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = mahasiswa_cookie(&db, &state).await;
    insert_periode(&db, "-30 days", "-20 days").await;

    assert!(ids(&app, URI, &cookie).await.is_empty());
}

#[sqlx::test]
async fn rejects_invalid_query_values(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = tim_mb_cookie(&db, &state).await;

    for query in ["status=aktif", "status=", "id=abc"] {
        let uri = format!("{URI}?{query}");
        let response = get(&app, &uri, Some(&cookie)).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri}");
    }
}

#[sqlx::test]
async fn needs_a_token_and_a_known_role(db: PgPool) {
    let (app, state) = setup(db.clone());
    assert_eq!(
        get(&app, URI, None).await.status(),
        StatusCode::UNAUTHORIZED
    );

    let user = insert_pengguna(&db, "tanpa.peran@gmail.com").await;
    let cookie = cookie_for(&state, &user);
    assert_eq!(
        get(&app, URI, Some(&cookie)).await.status(),
        StatusCode::FORBIDDEN
    );
}

async fn post_periode(app: &Router, cookie: Option<&str>, body: Value) -> Response<Body> {
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

async fn periode_count(db: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM periode_pendaftaran")
        .fetch_one(db)
        .await
        .unwrap()
}

fn range(start: &str, end: &str) -> Value {
    json!({ "waktu_mulai": start, "waktu_tutup": end })
}

#[sqlx::test]
async fn tim_mb_creates_a_period(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = tim_mb_cookie(&db, &state).await;

    let response = post_periode(
        &app,
        Some(&cookie),
        json!({
            "catatan": "  Genap 2090  ",
            "waktu_mulai": "2090-01-05T08:30:00+07:00",
            "waktu_tutup": "2090-02-15T23:59:00+07:00",
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let created = json_body(response).await;
    let id = created["id_periode"].as_i64().unwrap() as i32;
    assert_eq!(
        created,
        json!({
            "id_periode": id,
            "catatan": "Genap 2090",
            "waktu_mulai": "2090-01-05T08:30:00+07:00",
            "waktu_tutup": "2090-02-15T23:59:00+07:00",
            "status": "belum_dimulai",
        })
    );
    assert_eq!(ids(&app, URI, &cookie).await, [id]);
}

#[sqlx::test]
async fn times_in_other_zones_are_saved_in_wib(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = tim_mb_cookie(&db, &state).await;

    let response = post_periode(
        &app,
        Some(&cookie),
        range("2090-01-05T01:30:00.000Z", "2090-01-05T18:00:00-05:00"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let created = json_body(response).await;
    assert_eq!(created["waktu_mulai"], "2090-01-05T08:30:00+07:00");
    assert_eq!(created["waktu_tutup"], "2090-01-06T06:00:00+07:00");
}

#[sqlx::test]
async fn catatan_is_optional(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = tim_mb_cookie(&db, &state).await;

    let bodies = [
        range("2090-01-01T00:00:00+07:00", "2090-01-02T00:00:00+07:00"),
        json!({
            "catatan": null,
            "waktu_mulai": "2090-02-01T00:00:00+07:00",
            "waktu_tutup": "2090-02-02T00:00:00+07:00",
        }),
        json!({
            "catatan": "   ",
            "waktu_mulai": "2090-03-01T00:00:00+07:00",
            "waktu_tutup": "2090-03-02T00:00:00+07:00",
        }),
    ];

    for body in bodies {
        let response = post_periode(&app, Some(&cookie), body.clone()).await;
        assert_eq!(response.status(), StatusCode::CREATED, "{body}");
        assert_eq!(json_body(response).await["catatan"], Value::Null, "{body}");
    }
}

#[sqlx::test]
async fn rejects_invalid_values_without_saving(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = tim_mb_cookie(&db, &state).await;
    let start = "2090-01-01T00:00:00+07:00";
    let end = "2090-01-02T00:00:00+07:00";

    let with_message = [
        range(end, start),
        range(start, start),
        // The same instant written in two zones is still not a valid range.
        range("2090-01-01T00:00:00+07:00", "2089-12-31T17:00:00Z"),
        range("2090-01-01T00:00:00", end),
        range("2090-01-01", end),
        range("besok", end),
        range(start, "2090-02-31T00:00:00+07:00"),
        json!({ "catatan": "x".repeat(256), "waktu_mulai": start, "waktu_tutup": end }),
    ];
    for body in with_message {
        let response = post_periode(&app, Some(&cookie), body.clone()).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}"
        );
        assert!(json_body(response).await["message"].is_string(), "{body}");
    }

    let from_framework = [
        json!({}),
        json!({ "waktu_mulai": start }),
        json!({ "waktu_mulai": null, "waktu_tutup": end }),
        json!({ "waktu_mulai": start, "waktu_tutup": end, "id_periode": 1 }),
    ];
    for body in from_framework {
        let response = post_periode(&app, Some(&cookie), body.clone()).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}"
        );
    }

    assert_eq!(periode_count(&db).await, 0);
}

#[sqlx::test]
async fn rejects_ranges_touching_an_existing_period(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = tim_mb_cookie(&db, &state).await;
    let first = post_periode(
        &app,
        Some(&cookie),
        range("2090-03-10T00:00:00+07:00", "2090-03-20T00:00:00+07:00"),
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);

    let overlapping = [
        (
            "inside",
            "2090-03-12T00:00:00+07:00",
            "2090-03-15T00:00:00+07:00",
        ),
        (
            "around",
            "2090-03-01T00:00:00+07:00",
            "2090-03-30T00:00:00+07:00",
        ),
        (
            "over the start",
            "2090-03-01T00:00:00+07:00",
            "2090-03-11T00:00:00+07:00",
        ),
        (
            "over the end",
            "2090-03-19T00:00:00+07:00",
            "2090-03-30T00:00:00+07:00",
        ),
        (
            "ends at the start",
            "2090-03-01T00:00:00+07:00",
            "2090-03-10T00:00:00+07:00",
        ),
        (
            "starts at the end",
            "2090-03-20T00:00:00+07:00",
            "2090-03-30T00:00:00+07:00",
        ),
        (
            "overlap in another zone",
            "2090-03-19T16:00:00Z",
            "2090-03-30T00:00:00Z",
        ),
    ];
    for (name, start, end) in overlapping {
        let response = post_periode(&app, Some(&cookie), range(start, end)).await;
        assert_eq!(response.status(), StatusCode::CONFLICT, "{name}");
        assert!(json_body(response).await["message"].is_string(), "{name}");
    }
    assert_eq!(periode_count(&db).await, 1);

    let adjacent = post_periode(
        &app,
        Some(&cookie),
        range("2090-03-20T00:00:01+07:00", "2090-03-30T00:00:00+07:00"),
    )
    .await;
    assert_eq!(adjacent.status(), StatusCode::CREATED);
}

#[sqlx::test]
async fn only_one_of_several_concurrent_overlapping_requests_wins(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = tim_mb_cookie(&db, &state).await;
    let body = range("2090-01-01T00:00:00+07:00", "2090-01-02T00:00:00+07:00");

    let (a, b, c, d) = tokio::join!(
        post_periode(&app, Some(&cookie), body.clone()),
        post_periode(&app, Some(&cookie), body.clone()),
        post_periode(&app, Some(&cookie), body.clone()),
        post_periode(&app, Some(&cookie), body.clone()),
    );

    let created = [a, b, c, d]
        .iter()
        .filter(|response| response.status() == StatusCode::CREATED)
        .count();
    assert_eq!(created, 1);
    assert_eq!(periode_count(&db).await, 1);
}

#[sqlx::test]
async fn only_tim_mb_can_create_periods(db: PgPool) {
    let (app, state) = setup(db.clone());
    let body = range("2090-01-01T00:00:00+07:00", "2090-01-02T00:00:00+07:00");

    let without_token = post_periode(&app, None, body.clone()).await;
    assert_eq!(without_token.status(), StatusCode::UNAUTHORIZED);

    let mahasiswa = mahasiswa_cookie(&db, &state).await;
    let response = post_periode(&app, Some(&mahasiswa), body.clone()).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let user = insert_pengguna(&db, "tanpa.peran@gmail.com").await;
    let no_role = cookie_for(&state, &user);
    let response = post_periode(&app, Some(&no_role), body).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    assert_eq!(periode_count(&db).await, 0);
}

#[sqlx::test]
async fn the_middleware_only_guards_post_so_mahasiswa_can_still_read(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = mahasiswa_cookie(&db, &state).await;

    assert!(ids(&app, URI, &cookie).await.is_empty());
}
