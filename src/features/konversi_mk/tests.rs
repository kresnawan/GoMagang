use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, Response, StatusCode, header},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

use crate::{app, config::Config, features::auth::User, state::AppState};

const MK: &str = "/api/v1/mata-kuliah";

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

async fn mahasiswa(db: &PgPool, state: &AppState, email: &str) -> (User, String) {
    let user = insert_pengguna(db, email).await;
    sqlx::query("INSERT INTO mahasiswa (id_pengguna) VALUES ($1)")
        .bind(user.id_pengguna)
        .execute(db)
        .await
        .unwrap();
    let cookie = cookie_for(state, &user);
    (user, cookie)
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

/// A pendaftaran of this mahasiswa with the given status, on a fresh position.
async fn pendaftaran(db: &PgPool, user: &User, status: &str) -> i64 {
    let id: i32 = sqlx::query_scalar(
        "WITH perusahaan AS (
             INSERT INTO perusahaan_mitra (nama_perusahaan) VALUES ('PT Satu')
             RETURNING id_perusahaan
         ), periode AS (
             INSERT INTO periode_pendaftaran (waktu_mulai, waktu_tutup)
             VALUES ('2026-01-01', '2026-12-31')
             RETURNING id_periode
         ), posisi AS (
             INSERT INTO posisi_magang
                 (id_perusahaan, posisi, deskripsi_persyaratan, kuota, id_periode)
             SELECT id_perusahaan, 'Backend', '-', 1, id_periode FROM perusahaan, periode
             RETURNING id_posisi
         )
         INSERT INTO pendaftaran (id_posisi, id_status, id_pengguna)
         SELECT id_posisi, id_status, $1
         FROM posisi, status_pendaftaran
         WHERE status = $2
         RETURNING id_pendaftaran",
    )
    .bind(user.id_pengguna)
    .bind(status)
    .fetch_one(db)
    .await
    .unwrap();
    id.into()
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

async fn json_body(response: Response<Body>) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn mk(asal: &str, konversi: &str, sks: i64) -> Value {
    json!({ "mk_asal": asal, "mk_konversi": konversi, "sks": sks })
}

async fn create_ok(app: &Router, cookie: &str, body: Value) -> i64 {
    let response = send(app, "POST", MK, Some(cookie), Some(body)).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    json_body(response).await["id_mk_konversi"]
        .as_i64()
        .unwrap()
}

async fn list_mk(app: &Router, cookie: &str, query: &str) -> Value {
    let response = send(app, "GET", &format!("{MK}{query}"), Some(cookie), None).await;
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

fn penyetaraan_uri(id_pendaftaran: i64) -> String {
    format!("/api/v1/pendaftaran/{id_pendaftaran}/penyetaraan-mk")
}

async fn mk_count(db: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM mata_kuliah_konversi")
        .fetch_one(db)
        .await
        .unwrap()
}

// ---- Katalog mata kuliah ----

#[sqlx::test]
async fn panitia_magang_creates_a_mata_kuliah_and_everyone_logged_in_can_list_it(db: PgPool) {
    let (app, state) = setup(db.clone());
    let panitia_magang = panitia_magang_cookie(&db, &state).await;
    let (_, mahasiswa) = mahasiswa(&db, &state, "m@student.ub.ac.id").await;

    let response = send(
        &app,
        "POST",
        MK,
        Some(&panitia_magang),
        Some(mk("  Pengembangan Web  ", " Pemrograman Web ", 3)),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let created = json_body(response).await;
    assert!(created["id_mk_konversi"].is_i64());
    assert_eq!(created["mk_asal"], "Pengembangan Web");
    assert_eq!(created["mk_konversi"], "Pemrograman Web");
    assert_eq!(created["sks"], 3);
    assert!(created["deleted_at"].is_null());
    assert_eq!(created.as_object().unwrap().len(), 5);

    for cookie in [&panitia_magang, &mahasiswa] {
        assert_eq!(list_mk(&app, cookie, "").await, json!([created]));
    }
}

#[sqlx::test]
async fn lists_mata_kuliah_by_konversi_name(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = panitia_magang_cookie(&db, &state).await;
    assert_eq!(list_mk(&app, &cookie, "").await, json!([]));

    for konversi in ["Data Mining", "Basis Data", "Etika Profesi"] {
        create_ok(&app, &cookie, mk("Asal", konversi, 2)).await;
    }

    let list = list_mk(&app, &cookie, "").await;
    let names: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["mk_konversi"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Basis Data", "Data Mining", "Etika Profesi"]);
}

#[sqlx::test]
async fn rejects_a_pair_that_already_exists_but_allows_either_name_alone(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = panitia_magang_cookie(&db, &state).await;
    create_ok(&app, &cookie, mk("Web", "Pemrograman Web", 3)).await;

    for body in [
        mk("Web", "Pemrograman Web", 3),
        mk("web", "PEMROGRAMAN WEB", 2),
        mk("  Web ", "Pemrograman Web  ", 3),
    ] {
        let response = send(&app, "POST", MK, Some(&cookie), Some(body.clone())).await;
        assert_eq!(response.status(), StatusCode::CONFLICT, "{body}");
        assert!(json_body(response).await["message"].is_string());
    }

    create_ok(&app, &cookie, mk("Web Lanjut", "Pemrograman Web", 3)).await;
    create_ok(&app, &cookie, mk("Web", "Desain Web", 3)).await;
    assert_eq!(mk_count(&db).await, 3);
}

#[sqlx::test]
async fn only_one_of_several_concurrent_requests_with_the_same_pair_wins(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = panitia_magang_cookie(&db, &state).await;
    let body = mk("Web", "Pemrograman Web", 3);

    let (a, b, c, d) = tokio::join!(
        send(&app, "POST", MK, Some(&cookie), Some(body.clone())),
        send(&app, "POST", MK, Some(&cookie), Some(body.clone())),
        send(&app, "POST", MK, Some(&cookie), Some(body.clone())),
        send(&app, "POST", MK, Some(&cookie), Some(body.clone())),
    );

    let created = [a, b, c, d]
        .iter()
        .filter(|response| response.status() == StatusCode::CREATED)
        .count();
    assert_eq!(created, 1);
    assert_eq!(mk_count(&db).await, 1);
}

#[sqlx::test]
async fn rejects_invalid_values_without_saving(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = panitia_magang_cookie(&db, &state).await;

    for body in [
        mk("  ", "Pemrograman Web", 3),
        mk("Web", "", 3),
        mk(&"x".repeat(256), "Pemrograman Web", 3),
        mk("Web", &"x".repeat(256), 3),
        mk("Web", "Pemrograman Web", 0),
        mk("Web", "Pemrograman Web", 25),
        mk("Web", "Pemrograman Web", -1),
        json!({ "mk_asal": "Web", "mk_konversi": "Pemrograman Web", "sks": "3" }),
        json!({ "mk_asal": "Web", "mk_konversi": "Pemrograman Web" }),
        json!({ "mk_asal": "Web", "mk_konversi": "Web", "sks": 3, "lain": 1 }),
    ] {
        let response = send(&app, "POST", MK, Some(&cookie), Some(body.clone())).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}"
        );
    }
    assert_eq!(mk_count(&db).await, 0);

    for sks in [1, 24] {
        create_ok(&app, &cookie, mk("Web", &format!("MK {sks}"), sks)).await;
    }
}

#[sqlx::test]
async fn only_panitia_magang_can_create_or_hide_and_listing_needs_a_role(db: PgPool) {
    let (app, state) = setup(db.clone());
    let panitia_magang = panitia_magang_cookie(&db, &state).await;
    let (_, mahasiswa) = mahasiswa(&db, &state, "m@student.ub.ac.id").await;
    let tanpa_peran = cookie_for(&state, &insert_pengguna(&db, "x@gmail.com").await);
    let id = create_ok(&app, &panitia_magang, mk("Web", "Pemrograman Web", 3)).await;

    for (cookie, expected) in [
        (None, StatusCode::UNAUTHORIZED),
        (Some(mahasiswa.as_str()), StatusCode::FORBIDDEN),
        (Some(tanpa_peran.as_str()), StatusCode::FORBIDDEN),
    ] {
        let response = send(&app, "POST", MK, cookie, Some(mk("A", "B", 2))).await;
        assert_eq!(response.status(), expected);
        let response = send(&app, "DELETE", &format!("{MK}/{id}"), cookie, None).await;
        assert_eq!(response.status(), expected);
    }
    assert_eq!(mk_count(&db).await, 1);
    assert_eq!(
        list_mk(&app, &panitia_magang, "")
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let response = send(&app, "GET", MK, None, None).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = send(&app, "GET", MK, Some(&tanpa_peran), None).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = send(
        &app,
        "GET",
        &format!("{MK}?termasuk_dihapus=true"),
        Some(&mahasiswa),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn hiding_takes_it_out_of_the_list_keeps_its_row_and_frees_the_pair(db: PgPool) {
    let (app, state) = setup(db.clone());
    let cookie = panitia_magang_cookie(&db, &state).await;
    let id = create_ok(&app, &cookie, mk("Web", "Pemrograman Web", 3)).await;
    let lain = create_ok(&app, &cookie, mk("Data", "Basis Data", 2)).await;

    let response = send(&app, "DELETE", &format!("{MK}/{id}"), Some(&cookie), None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let hidden = json_body(response).await;
    assert_eq!(hidden["id_mk_konversi"], id);
    assert!(hidden["deleted_at"].as_str().unwrap().ends_with("+07:00"));

    let list = list_mk(&app, &cookie, "").await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id_mk_konversi"], lain);

    // Hidden ones come after the others.
    let semua = list_mk(&app, &cookie, "?termasuk_dihapus=true").await;
    assert_eq!(semua[0]["id_mk_konversi"], lain);
    assert_eq!(semua[1], hidden);
    assert_eq!(mk_count(&db).await, 2);

    let response = send(&app, "DELETE", &format!("{MK}/{id}"), Some(&cookie), None).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = send(&app, "DELETE", &format!("{MK}/99999"), Some(&cookie), None).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    create_ok(&app, &cookie, mk("Web", "Pemrograman Web", 3)).await;
}

// ---- Penyetaraan ----

#[sqlx::test]
async fn mahasiswa_chooses_and_drops_mata_kuliah_for_their_aktif_pendaftaran(db: PgPool) {
    let (app, state) = setup(db.clone());
    let panitia_magang = panitia_magang_cookie(&db, &state).await;
    let (user, cookie) = mahasiswa(&db, &state, "m@student.ub.ac.id").await;
    let id_pendaftaran = pendaftaran(&db, &user, "Aktif").await;
    let id_mk = create_ok(&app, &panitia_magang, mk("Web", "Pemrograman Web", 3)).await;
    let uri = penyetaraan_uri(id_pendaftaran);

    let response = send(&app, "GET", &uri, Some(&cookie), None).await;
    assert_eq!(json_body(response).await, json!([]));

    let body = json!({ "id_mk_konversi": id_mk });
    let response = send(&app, "POST", &uri, Some(&cookie), Some(body.clone())).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let created = json_body(response).await;
    assert_eq!(created["id_pendaftaran"], id_pendaftaran);
    assert!(created["created_at"].as_str().unwrap().ends_with("+07:00"));
    assert_eq!(created["mata_kuliah"]["id_mk_konversi"], id_mk);
    assert_eq!(created["mata_kuliah"]["sks"], 3);

    let response = send(&app, "POST", &uri, Some(&cookie), Some(body)).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    // Panitia Magang can see it too.
    for cookie in [&cookie, &panitia_magang] {
        let response = send(&app, "GET", &uri, Some(cookie), None).await;
        assert_eq!(json_body(response).await, json!([created]));
    }

    let hapus = format!("/api/v1/penyetaraan-mk/{}", created["id_penyetaraan"]);
    let response = send(&app, "DELETE", &hapus, Some(&cookie), None).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = send(&app, "DELETE", &hapus, Some(&cookie), None).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = send(&app, "GET", &uri, Some(&cookie), None).await;
    assert_eq!(json_body(response).await, json!([]));
}

#[sqlx::test]
async fn a_hidden_mata_kuliah_cannot_be_chosen_but_stays_where_it_was_chosen(db: PgPool) {
    let (app, state) = setup(db.clone());
    let panitia_magang = panitia_magang_cookie(&db, &state).await;
    let (user, cookie) = mahasiswa(&db, &state, "m@student.ub.ac.id").await;
    let uri = penyetaraan_uri(pendaftaran(&db, &user, "Aktif").await);
    let dipilih = create_ok(&app, &panitia_magang, mk("Web", "Pemrograman Web", 3)).await;
    let belum = create_ok(&app, &panitia_magang, mk("Data", "Basis Data", 2)).await;

    send(
        &app,
        "POST",
        &uri,
        Some(&cookie),
        Some(json!({ "id_mk_konversi": dipilih })),
    )
    .await;
    for id in [dipilih, belum] {
        send(
            &app,
            "DELETE",
            &format!("{MK}/{id}"),
            Some(&panitia_magang),
            None,
        )
        .await;
    }

    let response = send(
        &app,
        "POST",
        &uri,
        Some(&cookie),
        Some(json!({ "id_mk_konversi": belum })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let list = json_body(send(&app, "GET", &uri, Some(&cookie), None).await).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["mata_kuliah"]["id_mk_konversi"], dipilih);
    assert!(list[0]["mata_kuliah"]["deleted_at"].is_string());

    // It can still be dropped.
    let hapus = format!("/api/v1/penyetaraan-mk/{}", list[0]["id_penyetaraan"]);
    let response = send(&app, "DELETE", &hapus, Some(&cookie), None).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[sqlx::test]
async fn only_an_aktif_pendaftaran_can_change_its_mata_kuliah(db: PgPool) {
    let (app, state) = setup(db.clone());
    let panitia_magang = panitia_magang_cookie(&db, &state).await;
    let (user, cookie) = mahasiswa(&db, &state, "m@student.ub.ac.id").await;
    let id_mk = create_ok(&app, &panitia_magang, mk("Web", "Pemrograman Web", 3)).await;
    let id_pendaftaran = pendaftaran(&db, &user, "Diterima").await;

    let response = send(
        &app,
        "POST",
        &penyetaraan_uri(id_pendaftaran),
        Some(&cookie),
        Some(json!({ "id_mk_konversi": id_mk })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let id_penyetaraan: i32 = sqlx::query_scalar(
        "INSERT INTO penyetaraan_mk (id_mk_konversi, id_pendaftaran) VALUES ($1, $2)
         RETURNING id_penyetaraan",
    )
    .bind(id_mk as i32)
    .bind(id_pendaftaran as i32)
    .fetch_one(&db)
    .await
    .unwrap();
    let response = send(
        &app,
        "DELETE",
        &format!("/api/v1/penyetaraan-mk/{id_penyetaraan}"),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[sqlx::test]
async fn a_mahasiswa_cannot_touch_another_ones_penyetaraan(db: PgPool) {
    let (app, state) = setup(db.clone());
    let panitia_magang = panitia_magang_cookie(&db, &state).await;
    let (pemilik, pemilik_cookie) = mahasiswa(&db, &state, "a@student.ub.ac.id").await;
    let (_, lain) = mahasiswa(&db, &state, "b@student.ub.ac.id").await;
    let id_mk = create_ok(&app, &panitia_magang, mk("Web", "Pemrograman Web", 3)).await;
    let uri = penyetaraan_uri(pendaftaran(&db, &pemilik, "Aktif").await);
    let body = json!({ "id_mk_konversi": id_mk });

    let created = json_body(
        send(
            &app,
            "POST",
            &uri,
            Some(&pemilik_cookie),
            Some(body.clone()),
        )
        .await,
    )
    .await;
    let hapus = format!("/api/v1/penyetaraan-mk/{}", created["id_penyetaraan"]);

    let response = send(&app, "GET", &uri, Some(&lain), None).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = send(&app, "POST", &uri, Some(&lain), Some(body.clone())).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = send(&app, "DELETE", &hapus, Some(&lain), None).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // Panitia Magang only reads penyetaraan.
    let response = send(&app, "POST", &uri, Some(&panitia_magang), Some(body)).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = send(&app, "DELETE", &hapus, Some(&panitia_magang), None).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = send(
        &app,
        "GET",
        &penyetaraan_uri(99999),
        Some(&panitia_magang),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = send(&app, "GET", &uri, None, None).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn concurrent_requests_choose_a_mata_kuliah_only_once(db: PgPool) {
    let (app, state) = setup(db.clone());
    let panitia_magang = panitia_magang_cookie(&db, &state).await;
    let (user, cookie) = mahasiswa(&db, &state, "m@student.ub.ac.id").await;
    let id_mk = create_ok(&app, &panitia_magang, mk("Web", "Pemrograman Web", 3)).await;
    let uri = penyetaraan_uri(pendaftaran(&db, &user, "Aktif").await);
    let body = Some(json!({ "id_mk_konversi": id_mk }));

    let (a, b, c) = tokio::join!(
        send(&app, "POST", &uri, Some(&cookie), body.clone()),
        send(&app, "POST", &uri, Some(&cookie), body.clone()),
        send(&app, "POST", &uri, Some(&cookie), body.clone()),
    );
    let created = [a, b, c]
        .iter()
        .filter(|response| response.status() == StatusCode::CREATED)
        .count();
    assert_eq!(created, 1);
}
