use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, Response, StatusCode, header},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

use crate::{app, config::Config, features::auth::User, state::AppState};

const URI: &str = "/api/v1/posisi";

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

async fn mahasiswa_cookie(db: &PgPool, state: &AppState) -> String {
    let user = insert_pengguna(db, "m@student.ub.ac.id").await;
    sqlx::query("INSERT INTO mahasiswa (id_pengguna) VALUES ($1)")
        .bind(user.id_pengguna)
        .execute(db)
        .await
        .unwrap();
    cookie_for(state, &user)
}

async fn panitia_magang_cookie(db: &PgPool, state: &AppState) -> String {
    let user = insert_pengguna(db, "panitia@gmail.com").await;
    sqlx::query("INSERT INTO panitia_magang (id_pengguna) VALUES ($1)")
        .bind(user.id_pengguna)
        .execute(db)
        .await
        .unwrap();
    cookie_for(state, &user)
}

fn cookie_for(state: &AppState, user: &User) -> String {
    let cookie = state.sessions.access_cookie(user).unwrap();
    format!("{}={}", cookie.name(), cookie.value())
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

async fn insert_perusahaan(db: &PgPool, nama: &str) -> i32 {
    sqlx::query_scalar(
        "INSERT INTO perusahaan_mitra (nama_perusahaan) VALUES ($1) RETURNING id_perusahaan",
    )
    .bind(nama)
    .fetch_one(db)
    .await
    .unwrap()
}

async fn insert_posisi(
    db: &PgPool,
    id_perusahaan: i32,
    id_periode: i32,
    posisi: &str,
    kuota: i32,
) -> i32 {
    sqlx::query_scalar(
        "INSERT INTO posisi_magang
             (id_perusahaan, posisi, deskripsi_persyaratan, kuota, id_periode)
         VALUES ($1, $2, 'Syarat', $3, $4)
         RETURNING id_posisi",
    )
    .bind(id_perusahaan)
    .bind(posisi)
    .bind(kuota)
    .bind(id_periode)
    .fetch_one(db)
    .await
    .unwrap()
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

/// The names of the positions in a successful response, in order.
async fn names(app: &Router, uri: &str, cookie: &str) -> Vec<String> {
    let response = get(app, uri, Some(cookie)).await;
    assert_eq!(response.status(), StatusCode::OK, "{uri}");
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let list: Vec<Value> = serde_json::from_slice(&body).unwrap();
    list.iter()
        .map(|item| item["posisi"].as_str().unwrap().to_string())
        .collect()
}

struct Periods {
    past: i32,
    running: i32,
    future: i32,
}

async fn seed_periods(db: &PgPool) -> Periods {
    let past = insert_periode(db, "-30 days", "-20 days").await;
    let running = insert_periode(db, "-1 minute", "1 minute").await;
    let future = insert_periode(db, "20 days", "30 days").await;
    let company = insert_perusahaan(db, "PT Satu").await;
    insert_posisi(db, company, past, "Lama", 1).await;
    insert_posisi(db, company, running, "Berjalan", 1).await;
    insert_posisi(db, company, future, "Akan Datang", 1).await;
    Periods {
        past,
        running,
        future,
    }
}

#[sqlx::test]
async fn returns_the_position_with_its_company(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = mahasiswa_cookie(&db, &state).await;
    let periode = insert_periode(&db, "-1 minute", "1 minute").await;
    let company = insert_perusahaan(&db, "PT Satu").await;
    let posisi = insert_posisi(&db, company, periode, "Backend", 3).await;

    let response = get(&app, URI, Some(&cookie)).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let list: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        list,
        json!([{
            "id_posisi": posisi,
            "posisi": "Backend",
            "deskripsi_persyaratan": "Syarat",
            "kuota": 3,
            "id_periode": periode,
            "perusahaan": { "id_perusahaan": company, "nama_perusahaan": "PT Satu" },
            "deleted_at": null,
        }])
    );
}

#[sqlx::test]
async fn mahasiswa_only_sees_the_running_period(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = mahasiswa_cookie(&db, &state).await;
    let periods = seed_periods(&db).await;

    assert_eq!(names(&app, URI, &cookie).await, ["Berjalan"]);
    let running = format!("{URI}?id_periode={}", periods.running);
    assert_eq!(names(&app, &running, &cookie).await, ["Berjalan"]);

    for id in [periods.past, periods.future, 9999] {
        let uri = format!("{URI}?id_periode={id}");
        let response = get(&app, &uri, Some(&cookie)).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{uri}");
    }
}

#[sqlx::test]
async fn mahasiswa_gets_nothing_when_no_period_is_running(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = mahasiswa_cookie(&db, &state).await;
    let past = insert_periode(&db, "-30 days", "-20 days").await;
    let company = insert_perusahaan(&db, "PT Satu").await;
    insert_posisi(&db, company, past, "Lama", 1).await;

    assert!(names(&app, URI, &cookie).await.is_empty());
    let uri = format!("{URI}?id_periode={past}");
    let response = get(&app, &uri, Some(&cookie)).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn panitia_magang_defaults_to_the_running_period_and_can_pick_any(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = panitia_magang_cookie(&db, &state).await;
    let periods = seed_periods(&db).await;

    assert_eq!(names(&app, URI, &cookie).await, ["Berjalan"]);
    let past = format!("{URI}?id_periode={}", periods.past);
    assert_eq!(names(&app, &past, &cookie).await, ["Lama"]);
    let future = format!("{URI}?id_periode={}", periods.future);
    assert_eq!(names(&app, &future, &cookie).await, ["Akan Datang"]);
    let unknown = format!("{URI}?id_periode=9999");
    assert!(names(&app, &unknown, &cookie).await.is_empty());
}

#[sqlx::test]
async fn panitia_magang_defaults_to_the_latest_period_when_none_is_running(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = panitia_magang_cookie(&db, &state).await;
    let older = insert_periode(&db, "-30 days", "-20 days").await;
    let newer = insert_periode(&db, "-10 days", "-5 days").await;
    let company = insert_perusahaan(&db, "PT Satu").await;
    insert_posisi(&db, company, older, "Lama", 1).await;
    insert_posisi(&db, company, newer, "Terakhir", 1).await;

    assert_eq!(names(&app, URI, &cookie).await, ["Terakhir"]);
}

#[sqlx::test]
async fn returns_an_empty_list_when_there_are_no_periods(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = panitia_magang_cookie(&db, &state).await;

    assert!(names(&app, URI, &cookie).await.is_empty());
}

#[sqlx::test]
async fn the_quota_does_not_hide_any_position(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = mahasiswa_cookie(&db, &state).await;
    let periode = insert_periode(&db, "-1 minute", "1 minute").await;
    let company = insert_perusahaan(&db, "PT Satu").await;
    insert_posisi(&db, company, periode, "A", 1).await;
    insert_posisi(&db, company, periode, "B", 2).await;

    // is_full is gone along with the remaining quota, so it is ignored like any unknown query.
    for uri in [URI.to_string(), format!("{URI}?is_full=1")] {
        assert_eq!(names(&app, &uri, &cookie).await, ["A", "B"], "{uri}");
    }
}

#[sqlx::test]
async fn filters_by_company_and_name(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = mahasiswa_cookie(&db, &state).await;
    let periode = insert_periode(&db, "-1 minute", "1 minute").await;
    let first = insert_perusahaan(&db, "PT Satu").await;
    let second = insert_perusahaan(&db, "PT Dua").await;
    insert_posisi(&db, first, periode, "Backend Developer", 1).await;
    insert_posisi(&db, first, periode, "Data Analyst", 1).await;
    insert_posisi(&db, second, periode, "Frontend Developer", 1).await;

    let cases = [
        (
            format!("{URI}?p={first}"),
            vec!["Backend Developer", "Data Analyst"],
        ),
        (format!("{URI}?p=9999"), vec![]),
        (
            format!("{URI}?s=DEVELOPER"),
            vec!["Backend Developer", "Frontend Developer"],
        ),
        (format!("{URI}?s=%20analyst%20"), vec!["Data Analyst"]),
        (
            format!("{URI}?s="),
            vec!["Backend Developer", "Data Analyst", "Frontend Developer"],
        ),
        (format!("{URI}?s=%25"), vec![]),
        (format!("{URI}?s=_"), vec![]),
        (
            format!("{URI}?s=developer&p={second}"),
            vec!["Frontend Developer"],
        ),
    ];

    for (uri, expected) in cases {
        assert_eq!(names(&app, &uri, &cookie).await, expected, "{uri}");
    }
}

#[sqlx::test]
async fn rejects_invalid_query_values(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = mahasiswa_cookie(&db, &state).await;

    for query in ["p=abc", "id_periode=abc"] {
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

#[sqlx::test]
async fn the_old_route_is_gone(db: PgPool) {
    let (app, _) = setup(db);

    let response = get(&app, "/api/v1/lowongan", None).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

async fn post_posisi(app: &Router, cookie: Option<&str>, body: Value) -> Response<Body> {
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

async fn posisi_count(db: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM posisi_magang")
        .fetch_one(db)
        .await
        .unwrap()
}

fn new_posisi(id_perusahaan: i32, id_periode: i32) -> Value {
    json!({
        "id_perusahaan": id_perusahaan,
        "id_periode": id_periode,
        "posisi": "Backend Developer",
        "deskripsi_persyaratan": "Memahami REST API.",
        "kuota": 5,
    })
}

#[sqlx::test]
async fn panitia_magang_creates_a_position(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = panitia_magang_cookie(&db, &state).await;
    let periode = insert_periode(&db, "-1 minute", "1 minute").await;
    let company = insert_perusahaan(&db, "PT Satu").await;

    let response = post_posisi(
        &app,
        Some(&cookie),
        json!({
            "id_perusahaan": company,
            "id_periode": periode,
            "posisi": "  Backend Developer  ",
            "deskripsi_persyaratan": "  Memahami REST API.  ",
            "kuota": 5,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let created = json_body(response).await;
    assert_eq!(
        created,
        json!({
            "id_posisi": created["id_posisi"],
            "posisi": "Backend Developer",
            "deskripsi_persyaratan": "Memahami REST API.",
            "kuota": 5,
            "id_periode": periode,
            "perusahaan": { "id_perusahaan": company, "nama_perusahaan": "PT Satu" },
            "deleted_at": null,
        })
    );
    assert!(created["id_posisi"].is_i64());
    assert_eq!(names(&app, URI, &cookie).await, ["Backend Developer"]);
}

#[sqlx::test]
async fn a_new_position_shows_up_for_mahasiswa_in_the_running_period(db: PgPool) {
    let (app, state) = setup(db.clone());
    let panitia_magang = panitia_magang_cookie(&db, &state).await;
    let mahasiswa = mahasiswa_cookie(&db, &state).await;
    let periode = insert_periode(&db, "-1 minute", "1 minute").await;
    let company = insert_perusahaan(&db, "PT Satu").await;

    let response = post_posisi(&app, Some(&panitia_magang), new_posisi(company, periode)).await;
    assert_eq!(response.status(), StatusCode::CREATED);

    assert_eq!(names(&app, URI, &mahasiswa).await, ["Backend Developer"]);
}

#[sqlx::test]
async fn positions_can_be_added_to_any_period(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = panitia_magang_cookie(&db, &state).await;
    let periods = seed_periods(&db).await;
    let company = insert_perusahaan(&db, "PT Baru").await;

    for id in [periods.past, periods.running, periods.future] {
        let response = post_posisi(&app, Some(&cookie), new_posisi(company, id)).await;
        assert_eq!(response.status(), StatusCode::CREATED, "{id}");
    }
}

#[sqlx::test]
async fn rejects_invalid_values_without_saving(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = panitia_magang_cookie(&db, &state).await;
    let periode = insert_periode(&db, "-1 minute", "1 minute").await;
    let company = insert_perusahaan(&db, "PT Satu").await;
    let with = |field: &str, value: Value| {
        let mut body = new_posisi(company, periode);
        body[field] = value;
        body
    };

    let with_message = [
        with("posisi", json!("   ")),
        with("posisi", json!("x".repeat(256))),
        with("deskripsi_persyaratan", json!("   ")),
        with("kuota", json!(0)),
        with("kuota", json!(-1)),
        with("id_perusahaan", json!(9999)),
        with("id_periode", json!(9999)),
    ];
    for body in with_message {
        let response = post_posisi(&app, Some(&cookie), body.clone()).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}"
        );
        assert!(json_body(response).await["message"].is_string(), "{body}");
    }

    let from_framework = [
        json!({}),
        with("kuota", json!(1.5)),
        with("kuota", json!("5")),
        with("posisi", Value::Null),
        with("kuota_total", json!(5)),
        with("sisa_kuota", json!(100)),
    ];
    for body in from_framework {
        let response = post_posisi(&app, Some(&cookie), body.clone()).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}"
        );
    }

    assert_eq!(posisi_count(&db).await, 0);
}

#[sqlx::test]
async fn reports_which_reference_was_not_found(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = panitia_magang_cookie(&db, &state).await;
    let periode = insert_periode(&db, "-1 minute", "1 minute").await;
    let company = insert_perusahaan(&db, "PT Satu").await;

    let response = post_posisi(&app, Some(&cookie), new_posisi(9999, periode)).await;
    assert_eq!(
        json_body(response).await["message"],
        "Perusahaan tidak ditemukan."
    );
    let response = post_posisi(&app, Some(&cookie), new_posisi(company, 9999)).await;
    assert_eq!(
        json_body(response).await["message"],
        "Periode tidak ditemukan."
    );
}

#[sqlx::test]
async fn only_panitia_magang_can_create_positions(db: PgPool) {
    let (app, state) = setup(db.clone());
    let periode = insert_periode(&db, "-1 minute", "1 minute").await;
    let company = insert_perusahaan(&db, "PT Satu").await;
    let body = new_posisi(company, periode);

    let without_token = post_posisi(&app, None, body.clone()).await;
    assert_eq!(without_token.status(), StatusCode::UNAUTHORIZED);
    let bad_token = post_posisi(&app, Some("access_token=not-a-jwt"), body.clone()).await;
    assert_eq!(bad_token.status(), StatusCode::UNAUTHORIZED);

    let mahasiswa = mahasiswa_cookie(&db, &state).await;
    let response = post_posisi(&app, Some(&mahasiswa), body.clone()).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let user = insert_pengguna(&db, "tanpa.peran@gmail.com").await;
    let no_role = cookie_for(&state, &user);
    let response = post_posisi(&app, Some(&no_role), body).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    assert_eq!(posisi_count(&db).await, 0);
}

async fn hide_perusahaan(db: &PgPool, id_perusahaan: i32) {
    sqlx::query("UPDATE perusahaan_mitra SET deleted_at = now() WHERE id_perusahaan = $1")
        .bind(id_perusahaan)
        .execute(db)
        .await
        .unwrap();
}

#[sqlx::test]
async fn positions_of_a_hidden_company_are_left_out(db: PgPool) {
    let (app, state) = setup(db.clone());
    let mahasiswa = mahasiswa_cookie(&db, &state).await;
    let panitia_magang = panitia_magang_cookie(&db, &state).await;
    let periode = insert_periode(&db, "-1 minute", "1 minute").await;
    let tetap = insert_perusahaan(&db, "PT Tetap").await;
    let hilang = insert_perusahaan(&db, "PT Hilang").await;
    insert_posisi(&db, tetap, periode, "Backend", 1).await;
    insert_posisi(&db, hilang, periode, "Frontend", 1).await;

    hide_perusahaan(&db, hilang).await;

    for cookie in [&mahasiswa, &panitia_magang] {
        assert_eq!(names(&app, URI, cookie).await, ["Backend"]);
        assert_eq!(
            names(&app, &format!("{URI}?p={hilang}"), cookie).await,
            Vec::<String>::new()
        );
    }
    assert_eq!(posisi_count(&db).await, 2);
}

#[sqlx::test]
async fn a_hidden_company_takes_no_new_positions(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = panitia_magang_cookie(&db, &state).await;
    let periode = insert_periode(&db, "-1 minute", "1 minute").await;
    let company = insert_perusahaan(&db, "PT Satu").await;
    hide_perusahaan(&db, company).await;

    let response = post_posisi(
        &app,
        Some(&cookie),
        json!({
            "id_perusahaan": company,
            "id_periode": periode,
            "posisi": "Backend",
            "deskripsi_persyaratan": "Syarat",
            "kuota": 1,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        json_body(response).await["message"],
        "Perusahaan tidak ditemukan."
    );
    assert_eq!(posisi_count(&db).await, 0);
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

fn isi_baru() -> Value {
    json!({
        "posisi": "  Frontend Developer  ",
        "deskripsi_persyaratan": "  Menguasai React.  ",
        "kuota": 8,
    })
}

async fn ubah(app: &Router, cookie: Option<&str>, id: i32, body: Value) -> Response<Body> {
    send(app, "PATCH", &format!("{URI}/{id}"), cookie, Some(body)).await
}

async fn sembunyikan(app: &Router, cookie: Option<&str>, id: i32) -> Response<Body> {
    send(app, "DELETE", &format!("{URI}/{id}"), cookie, None).await
}

/// One running position of PT Satu, with the cookies of a mahasiswa and of Panitia Magang.
struct Katalog {
    app: Router,
    db: PgPool,
    mahasiswa: String,
    panitia_magang: String,
    periode: i32,
    company: i32,
}

async fn katalog(db: PgPool) -> Katalog {
    let (app, state) = setup(db.clone());
    Katalog {
        mahasiswa: mahasiswa_cookie(&db, &state).await,
        panitia_magang: panitia_magang_cookie(&db, &state).await,
        periode: insert_periode(&db, "-1 minute", "1 minute").await,
        company: insert_perusahaan(&db, "PT Satu").await,
        app,
        db,
    }
}

impl Katalog {
    async fn posisi(&self, nama: &str) -> i32 {
        insert_posisi(&self.db, self.company, self.periode, nama, 3).await
    }

    async fn stored(&self, id: i32) -> (String, String, i32, i32, i32, bool) {
        sqlx::query_as(
            "SELECT posisi, deskripsi_persyaratan, kuota, id_perusahaan, id_periode,
                    deleted_at IS NOT NULL
             FROM posisi_magang WHERE id_posisi = $1",
        )
        .bind(id)
        .fetch_one(&self.db)
        .await
        .unwrap()
    }
}

#[sqlx::test]
async fn panitia_magang_changes_the_name_description_and_quota(db: PgPool) {
    let k = katalog(db).await;
    let id = k.posisi("Backend").await;

    let response = ubah(&k.app, Some(&k.panitia_magang), id, isi_baru()).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await,
        json!({
            "id_posisi": id,
            "posisi": "Frontend Developer",
            "deskripsi_persyaratan": "Menguasai React.",
            "kuota": 8,
            "id_periode": k.periode,
            "perusahaan": { "id_perusahaan": k.company, "nama_perusahaan": "PT Satu" },
            "deleted_at": null,
        })
    );
    for cookie in [&k.mahasiswa, &k.panitia_magang] {
        assert_eq!(names(&k.app, URI, cookie).await, ["Frontend Developer"]);
    }
    assert_eq!(posisi_count(&k.db).await, 1);
}

#[sqlx::test]
async fn the_company_and_period_of_a_position_cannot_be_changed(db: PgPool) {
    let k = katalog(db).await;
    let id = k.posisi("Backend").await;
    let before = k.stored(id).await;
    let lain = insert_perusahaan(&k.db, "PT Lain").await;
    let periode_lain = insert_periode(&k.db, "1 day", "2 days").await;

    for (field, value) in [("id_perusahaan", lain), ("id_periode", periode_lain)] {
        let mut body = isi_baru();
        body[field] = json!(value);
        let response = ubah(&k.app, Some(&k.panitia_magang), id, body).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{field}"
        );
    }

    assert_eq!(k.stored(id).await, before);
}

#[sqlx::test]
async fn changing_rejects_bad_values_without_saving(db: PgPool) {
    let k = katalog(db).await;
    let id = k.posisi("Backend").await;
    let before = k.stored(id).await;

    let cases = [
        (
            "posisi",
            json!("   "),
            "Posisi harus berisi 1-255 karakter.",
        ),
        (
            "posisi",
            json!("x".repeat(256)),
            "Posisi harus berisi 1-255 karakter.",
        ),
        (
            "deskripsi_persyaratan",
            json!(" "),
            "Deskripsi persyaratan wajib diisi.",
        ),
        ("kuota", json!(0), "Kuota minimal 1."),
    ];
    for (field, value, message) in cases {
        let mut body = isi_baru();
        body[field] = value;
        let response = ubah(&k.app, Some(&k.panitia_magang), id, body).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{field}"
        );
        assert_eq!(json_body(response).await["message"], message, "{field}");
    }

    // A field left out is rejected too, as every field is replaced.
    let mut body = isi_baru();
    body.as_object_mut().unwrap().remove("kuota");
    let response = ubah(&k.app, Some(&k.panitia_magang), id, body).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let response = send(
        &k.app,
        "PATCH",
        &format!("{URI}/abc"),
        Some(&k.panitia_magang),
        Some(isi_baru()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let response = ubah(&k.app, Some(&k.panitia_magang), 999_999, isi_baru()).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    assert_eq!(k.stored(id).await, before);
}

#[sqlx::test]
async fn hiding_a_position_takes_it_out_of_the_catalog_but_keeps_its_row(db: PgPool) {
    let k = katalog(db).await;
    let tetap = k.posisi("Backend").await;
    let hilang = k.posisi("Frontend").await;

    let response = sembunyikan(&k.app, Some(&k.panitia_magang), hilang).await;
    assert_eq!(response.status(), StatusCode::OK);
    let hidden = json_body(response).await;
    assert_eq!(hidden["id_posisi"], hilang);
    assert_eq!(hidden["posisi"], "Frontend");
    let deleted_at = hidden["deleted_at"].as_str().unwrap();
    assert!(deleted_at.ends_with("+07:00"), "{deleted_at}");

    // The time is WIB, whatever the database server's own time zone.
    let is_wib: bool = sqlx::query_scalar(
        "SELECT deleted_at BETWEEN (now() AT TIME ZONE 'Asia/Jakarta') - interval '1 minute'
                            AND (now() AT TIME ZONE 'Asia/Jakarta') + interval '1 minute'
         FROM posisi_magang WHERE id_posisi = $1",
    )
    .bind(hilang)
    .fetch_one(&k.db)
    .await
    .unwrap();
    assert!(is_wib);

    for cookie in [&k.mahasiswa, &k.panitia_magang] {
        assert_eq!(names(&k.app, URI, cookie).await, ["Backend"]);
    }
    assert_eq!(posisi_count(&k.db).await, 2);
    let _ = tetap;
}

#[sqlx::test]
async fn panitia_magang_can_list_the_hidden_positions_after_the_others(db: PgPool) {
    let k = katalog(db).await;
    let awal = k.posisi("Analis").await;
    k.posisi("Zebra").await;
    k.posisi("Backend").await;
    assert_eq!(
        sembunyikan(&k.app, Some(&k.panitia_magang), awal)
            .await
            .status(),
        StatusCode::OK
    );

    let uri = format!("{URI}?termasuk_dihapus=true");
    assert_eq!(
        names(&k.app, &uri, &k.panitia_magang).await,
        ["Backend", "Zebra", "Analis"]
    );
    let list = json_body(get(&k.app, &uri, Some(&k.panitia_magang)).await).await;
    assert!(list[0]["deleted_at"].is_null());
    assert!(list[2]["deleted_at"].is_string());

    // The other filters still apply.
    let uri = format!("{URI}?termasuk_dihapus=true&s=analis");
    assert_eq!(names(&k.app, &uri, &k.panitia_magang).await, ["Analis"]);

    // Only asking for the hidden ones is Panitia Magang's alone.
    let response = get(
        &k.app,
        &format!("{URI}?termasuk_dihapus=true"),
        Some(&k.mahasiswa),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let uri = format!("{URI}?termasuk_dihapus=false");
    assert_eq!(
        names(&k.app, &uri, &k.mahasiswa).await,
        ["Backend", "Zebra"]
    );

    let response = get(
        &k.app,
        &format!("{URI}?termasuk_dihapus=ya"),
        Some(&k.panitia_magang),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn a_hidden_position_cannot_be_hidden_or_changed_again(db: PgPool) {
    let k = katalog(db).await;
    let id = k.posisi("Backend").await;
    assert_eq!(
        sembunyikan(&k.app, Some(&k.panitia_magang), id)
            .await
            .status(),
        StatusCode::OK
    );
    let before = k.stored(id).await;

    assert_eq!(
        sembunyikan(&k.app, Some(&k.panitia_magang), id)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        sembunyikan(&k.app, Some(&k.panitia_magang), 999_999)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        ubah(&k.app, Some(&k.panitia_magang), id, isi_baru())
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    let response = send(
        &k.app,
        "DELETE",
        &format!("{URI}/abc"),
        Some(&k.panitia_magang),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    assert_eq!(k.stored(id).await, before);
}

#[sqlx::test]
async fn a_position_of_a_hidden_company_cannot_be_changed_or_hidden(db: PgPool) {
    let k = katalog(db).await;
    let id = k.posisi("Backend").await;
    hide_perusahaan(&k.db, k.company).await;
    let before = k.stored(id).await;

    assert_eq!(
        ubah(&k.app, Some(&k.panitia_magang), id, isi_baru())
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        sembunyikan(&k.app, Some(&k.panitia_magang), id)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    // Nor listed, even with the hidden positions.
    let uri = format!("{URI}?termasuk_dihapus=true");
    assert_eq!(
        names(&k.app, &uri, &k.panitia_magang).await,
        Vec::<String>::new()
    );

    assert_eq!(k.stored(id).await, before);
}

#[sqlx::test]
async fn only_panitia_magang_can_change_or_hide_a_position(db: PgPool) {
    let k = katalog(db).await;
    let id = k.posisi("Backend").await;
    let user = insert_pengguna(&k.db, "tanpa.peran@gmail.com").await;
    let (_, state) = setup(k.db.clone());
    let no_role = cookie_for(&state, &user);
    let before = k.stored(id).await;

    assert_eq!(
        ubah(&k.app, None, id, isi_baru()).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        sembunyikan(&k.app, None, id).await.status(),
        StatusCode::UNAUTHORIZED
    );
    for cookie in [&k.mahasiswa, &no_role] {
        assert_eq!(
            ubah(&k.app, Some(cookie), id, isi_baru()).await.status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            sembunyikan(&k.app, Some(cookie), id).await.status(),
            StatusCode::FORBIDDEN
        );
    }

    assert_eq!(k.stored(id).await, before);
}
