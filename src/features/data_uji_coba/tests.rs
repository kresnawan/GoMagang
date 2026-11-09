use std::path::PathBuf;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, Response, StatusCode, header},
};
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgPool};
use tower::ServiceExt;

use crate::{app, config::Config, features::auth::User, state::AppState};

const URI: &str = "/api/v1/mahasiswa";
const SECRET: &str = "admin-secret-with-at-least-32-bytes!";
const TARGET: &str = "target@student.ub.ac.id";
const OTHER: &str = "other@student.ub.ac.id";
const SEED_URI: &str = "/api/v1/mahasiswa/seed";

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Ctx {
    app: Router,
    state: AppState,
    db: PgPool,
    // Everything uploaded goes in the "uploads" folder inside this one, so a test can put a file
    // right outside the upload folder and still have it cleaned up.
    root: TempDir,
}

impl Ctx {
    fn uploads(&self) -> PathBuf {
        self.root.0.join("uploads")
    }

    fn has_file(&self, name: &str) -> bool {
        self.uploads().join(name).exists()
    }

    async fn count(&self, table: &str) -> i64 {
        sqlx::query_scalar(AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
            .fetch_one(&self.db)
            .await
            .unwrap()
    }

    async fn kuota(&self, id_posisi: i32) -> i32 {
        sqlx::query_scalar("SELECT kuota FROM posisi_magang WHERE id_posisi = $1")
            .bind(id_posisi)
            .fetch_one(&self.db)
            .await
            .unwrap()
    }

    async fn mahasiswa(&self, email: &str) -> User {
        let id_pengguna = sqlx::query_scalar(
            "INSERT INTO pengguna (email, google_sub) VALUES ($1, $1) RETURNING id_pengguna",
        )
        .bind(email)
        .fetch_one(&self.db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO mahasiswa (id_pengguna) VALUES ($1)")
            .bind(id_pengguna)
            .execute(&self.db)
            .await
            .unwrap();
        User {
            id_pengguna,
            email: email.to_string(),
        }
    }

    async fn tim_mb(&self) -> User {
        let id_pengguna = sqlx::query_scalar(
            "INSERT INTO pengguna (email, google_sub)
             VALUES ('tim@gmail.com', 'tim')
             RETURNING id_pengguna",
        )
        .fetch_one(&self.db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO tim_mb (id_pengguna) VALUES ($1)")
            .bind(id_pengguna)
            .execute(&self.db)
            .await
            .unwrap();
        User {
            id_pengguna,
            email: "tim@gmail.com".to_string(),
        }
    }

    fn cookie(&self, user: &User) -> String {
        let cookie = self.state.sessions.access_cookie(user).unwrap();
        format!("{}={}", cookie.name(), cookie.value())
    }

    async fn posisi(&self, kuota: i32) -> i32 {
        sqlx::query_scalar(
            "WITH pr AS (
                 INSERT INTO perusahaan_mitra (nama_perusahaan) VALUES ('PT Maju') RETURNING *
             ), pe AS (
                 INSERT INTO periode_pendaftaran (waktu_mulai, waktu_tutup)
                 VALUES (now() - interval '1 day', now() + interval '1 day')
                 RETURNING *
             )
             INSERT INTO posisi_magang
                 (id_perusahaan, posisi, deskripsi_persyaratan, kuota, id_periode)
             SELECT pr.id_perusahaan, 'Backend', '-', $1, pe.id_periode FROM pr, pe
             RETURNING id_posisi",
        )
        .bind(kuota)
        .fetch_one(&self.db)
        .await
        .unwrap()
    }

    /// A pendaftaran with its files on disk, a row in every table hanging off it, and a log row
    /// written by Tim MB if one is given. The berkas is named `{prefix}.pdf` and the surat
    /// keterangan `{prefix}-surat.pdf`.
    async fn pendaftaran(
        &self,
        user: &User,
        id_posisi: i32,
        prefix: &str,
        tim_mb: Option<i32>,
    ) -> i32 {
        let id: i32 = sqlx::query_scalar(
            "INSERT INTO pendaftaran (id_posisi, id_status, id_pengguna)
             SELECT $1, id_status, $2 FROM status_pendaftaran WHERE status = 'Menunggu Verifikasi'
             RETURNING id_pendaftaran",
        )
        .bind(id_posisi)
        .bind(user.id_pengguna)
        .fetch_one(&self.db)
        .await
        .unwrap();

        sqlx::query(
            "WITH log AS (
                 INSERT INTO log_status (id_pendaftaran, id_pengubah, status_baru)
                 SELECT $1, $3, id_status
                 FROM status_pendaftaran WHERE status = 'Menunggu Verifikasi'
             ), berkas AS (
                 INSERT INTO berkas_pendaftaran
                     (id_pendaftaran, jenis_berkas, nama_berkas, ukuran_berkas)
                 VALUES ($1, 'CV', $2, 4)
             ), jadwal AS (
                 INSERT INTO jadwal_seleksi_substansi (id_pendaftaran, tanggal, jam, lokasi)
                 VALUES ($1, '2026-12-01', '09:00', 'Malang')
             ), surat AS (
                 INSERT INTO surat_keterangan
                     (id_pendaftaran, nama_berkas, ukuran_berkas, nomor_surat, isi, tanda_tangan,
                      hash_berkas)
                 VALUES ($1, $4, 4, $4, '{}', 'tanda-tangan',
                         encode(sha256(convert_to($4, 'UTF8')), 'hex'))
             ), plotting AS (
                 INSERT INTO plotting_dpm (id_pendaftaran, nama_dpm, kontak_dpm)
                 VALUES ($1, 'Pak Dosen', '0812')
             ), mk AS (
                 INSERT INTO mata_kuliah_konversi (mk_konversi, mk_asal, sks)
                 VALUES ('Magang', 'Basis Data', 3)
                 RETURNING id_mk_konversi
             )
             INSERT INTO penyetaraan_mk (id_mk_konversi, id_pendaftaran)
             SELECT id_mk_konversi, $1 FROM mk",
        )
        .bind(id)
        .bind(format!("{prefix}.pdf"))
        .bind(tim_mb)
        .bind(format!("{prefix}-surat.pdf"))
        .execute(&self.db)
        .await
        .unwrap();

        for nama in [format!("{prefix}.pdf"), format!("{prefix}-surat.pdf")] {
            std::fs::write(self.uploads().join(nama), b"%PDF").unwrap();
        }
        id
    }

    async fn insert_refresh_token(&self, user: &User, hash: &[u8]) {
        sqlx::query(
            "INSERT INTO token_refresh (id_pengguna, id_keluarga, hash_token, expires_at)
             VALUES ($1, gen_random_uuid(), $2, now() + interval '1 day')",
        )
        .bind(user.id_pengguna)
        .bind(hash)
        .execute(&self.db)
        .await
        .unwrap();
    }

    /// How many rows of each table still point at this user, directly or through a pendaftaran.
    async fn rows_of(&self, id_pengguna: i32, id_pendaftaran: i32) -> i64 {
        let mut total = 0;
        for table in ["pengguna", "mahasiswa", "pendaftaran", "token_refresh"] {
            total += sqlx::query_scalar::<_, i64>(AssertSqlSafe(format!(
                "SELECT count(*) FROM {table} WHERE id_pengguna = $1"
            )))
            .bind(id_pengguna)
            .fetch_one(&self.db)
            .await
            .unwrap();
        }
        for table in [
            "log_status",
            "berkas_pendaftaran",
            "jadwal_seleksi_substansi",
            "surat_keterangan",
            "plotting_dpm",
            "penyetaraan_mk",
        ] {
            total += sqlx::query_scalar::<_, i64>(AssertSqlSafe(format!(
                "SELECT count(*) FROM {table} WHERE id_pendaftaran = $1"
            )))
            .bind(id_pendaftaran)
            .fetch_one(&self.db)
            .await
            .unwrap();
        }
        total
    }
}

fn setup_with(db: PgPool, admin_secret: Option<&str>) -> Ctx {
    let root = std::env::temp_dir().join(format!("gomagang-test-{:032x}", rand::random::<u128>()));
    let uploads = root.join("uploads");
    std::fs::create_dir_all(&uploads).unwrap();

    let config = Config {
        database_url: String::new(),
        cors_allowed_origins: vec![],
        google_client_id: "client".to_string(),
        google_client_secret: "secret".to_string(),
        google_redirect_uri: "http://backend.test/auth/google".to_string(),
        frontend_url: "http://frontend.test".to_string(),
        jwt_secret: "test-secret-with-at-least-32-bytes!".to_string(),
        admin_secret: admin_secret.map(str::to_string),
        cookie_secure: false,
        upload_dir: uploads,
        surat_signing_key: None,
    };
    let state = AppState::new(&config, db.clone());
    Ctx {
        app: app::build(&config, state.clone()),
        state,
        db,
        root: TempDir(root),
    }
}

fn setup(db: PgPool) -> Ctx {
    setup_with(db, Some(SECRET))
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    secret: Option<&str>,
    cookie: Option<&str>,
    body: Body,
) -> Response<Body> {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(secret) = secret {
        request = request.header("x-admin-secret", secret);
    }
    if let Some(cookie) = cookie {
        request = request
            .header(header::COOKIE, cookie)
            .header(header::CONTENT_TYPE, "application/json");
    }
    app.clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap()
}

async fn hapus(ctx: &Ctx) -> Response<Body> {
    send(&ctx.app, "DELETE", URI, Some(SECRET), None, Body::empty()).await
}

async fn seed(ctx: &Ctx) -> Response<Body> {
    send(
        &ctx.app,
        "POST",
        SEED_URI,
        Some(SECRET),
        None,
        Body::empty(),
    )
    .await
}

async fn get(ctx: &Ctx, cookie: &str, uri: &str) -> Response<Body> {
    send(&ctx.app, "GET", uri, None, Some(cookie), Body::empty()).await
}

async fn json_body(response: Response<Body>) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

impl Ctx {
    /// A position in a period running from `mulai` to `tutup` days from now.
    async fn posisi_pada(&self, mulai: i32, tutup: i32) -> i32 {
        let id_posisi = self.posisi(5).await;
        sqlx::query(
            "UPDATE periode_pendaftaran pp
             SET waktu_mulai = (now() AT TIME ZONE 'Asia/Jakarta') + make_interval(days => $2),
                 waktu_tutup = (now() AT TIME ZONE 'Asia/Jakarta') + make_interval(days => $3)
             FROM posisi_magang pm
             WHERE pm.id_periode = pp.id_periode AND pm.id_posisi = $1",
        )
        .bind(id_posisi)
        .bind(mulai)
        .bind(tutup)
        .execute(&self.db)
        .await
        .unwrap();
        id_posisi
    }

    /// The first seeded mahasiswa, as a user who can log in.
    async fn seeded_user(&self) -> User {
        let (id_pengguna, email) = sqlx::query_as(
            "SELECT p.id_pengguna, p.email::text
             FROM pengguna p
             JOIN mahasiswa m ON m.id_pengguna = p.id_pengguna
             ORDER BY p.id_pengguna
             LIMIT 1",
        )
        .fetch_one(&self.db)
        .await
        .unwrap();
        User { id_pengguna, email }
    }
}

#[sqlx::test]
async fn deleting_removes_every_mahasiswa_and_all_that_belongs_to_them(db: PgPool) {
    let ctx = setup(db);
    let tim_mb = ctx.tim_mb().await;
    let target = ctx.mahasiswa(TARGET).await;
    let other = ctx.mahasiswa(OTHER).await;
    let id_posisi = ctx.posisi(5).await;

    let first = ctx
        .pendaftaran(&target, id_posisi, "target-1", Some(tim_mb.id_pengguna))
        .await;
    let second = ctx.posisi(5).await;
    let second = ctx.pendaftaran(&target, second, "target-2", None).await;
    let theirs = ctx.pendaftaran(&other, id_posisi, "other", None).await;
    ctx.insert_refresh_token(&target, b"target").await;
    ctx.insert_refresh_token(&other, b"other").await;
    ctx.insert_refresh_token(&tim_mb, b"tim").await;

    assert!(ctx.rows_of(target.id_pengguna, first).await > 0);

    let response = hapus(&ctx).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await,
        json!({
            "jumlah_mahasiswa": 2,
            "jumlah_pendaftaran": 3,
            "jumlah_berkas": 6,
            "jumlah_berkas_gagal": 0,
        })
    );

    assert_eq!(ctx.rows_of(target.id_pengguna, first).await, 0);
    assert_eq!(ctx.rows_of(target.id_pengguna, second).await, 0);
    assert_eq!(ctx.rows_of(other.id_pengguna, theirs).await, 0);
    for table in [
        "mahasiswa",
        "pendaftaran",
        "log_status",
        "berkas_pendaftaran",
        "jadwal_seleksi_substansi",
        "surat_keterangan",
        "plotting_dpm",
        "penyetaraan_mk",
    ] {
        assert_eq!(ctx.count(table).await, 0, "{table}");
    }
    for file in [
        "target-1.pdf",
        "target-2.pdf",
        "other.pdf",
        "target-1-surat.pdf",
        "target-2-surat.pdf",
        "other-surat.pdf",
    ] {
        assert!(!ctx.has_file(file), "{file}");
    }

    // What is not a mahasiswa's stays, and Tim MB keeps their account and login.
    assert_eq!(ctx.count("pengguna").await, 1);
    assert_eq!(ctx.count("tim_mb").await, 1);
    assert_eq!(ctx.count("token_refresh").await, 1);
    assert_eq!(ctx.count("posisi_magang").await, 2);
    assert_eq!(ctx.count("periode_pendaftaran").await, 2);
    assert_eq!(ctx.count("perusahaan_mitra").await, 2);
    assert_eq!(ctx.count("mata_kuliah_konversi").await, 3);
    assert_eq!(ctx.count("status_pendaftaran").await, 7);
}

#[sqlx::test]
async fn deleting_leaves_the_positions_and_their_quota_alone(db: PgPool) {
    let ctx = setup(db);
    let target = ctx.mahasiswa(TARGET).await;
    let other = ctx.mahasiswa(OTHER).await;

    let dua = ctx.posisi(5).await;
    ctx.pendaftaran(&target, dua, "a", None).await;
    ctx.pendaftaran(&other, dua, "b", None).await;

    let untouched = ctx.posisi(5).await;

    assert_eq!(hapus(&ctx).await.status(), StatusCode::OK);

    assert_eq!(ctx.kuota(dua).await, 5);
    assert_eq!(ctx.kuota(untouched).await, 5);
    assert_eq!(ctx.count("posisi_magang").await, 2);
}

#[sqlx::test]
async fn deleting_with_nobody_left_changes_nothing(db: PgPool) {
    let ctx = setup(db);
    ctx.tim_mb().await;

    let response = hapus(&ctx).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await,
        json!({
            "jumlah_mahasiswa": 0,
            "jumlah_pendaftaran": 0,
            "jumlah_berkas": 0,
            "jumlah_berkas_gagal": 0,
        })
    );
    assert_eq!(ctx.count("tim_mb").await, 1);
}

#[sqlx::test]
async fn deleting_refuses_any_query_so_an_old_call_cannot_wipe_everyone(db: PgPool) {
    let ctx = setup(db);
    ctx.mahasiswa(TARGET).await;
    ctx.mahasiswa(OTHER).await;

    for query in [format!("?email={TARGET}"), "?semua=true".to_string()] {
        let uri = format!("{URI}{query}");
        let response = send(&ctx.app, "DELETE", &uri, Some(SECRET), None, Body::empty()).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{query}");
    }

    assert_eq!(ctx.count("mahasiswa").await, 2);
}

#[sqlx::test]
async fn both_routes_need_the_admin_secret(db: PgPool) {
    let ctx = setup(db);
    let target = ctx.mahasiswa(TARGET).await;
    let cookie = ctx.cookie(&target);
    let tim_mb = ctx.tim_mb().await;
    let tim_mb = ctx.cookie(&tim_mb);
    ctx.posisi(5).await;
    let pengguna = ctx.count("pengguna").await;

    for (method, uri) in [("DELETE", URI), ("POST", SEED_URI)] {
        for secret in [None, Some("salah")] {
            let response = send(&ctx.app, method, uri, secret, None, Body::empty()).await;
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{method}");
        }
        // A login, even Tim MB's, is no substitute for the secret.
        for cookie in [&cookie, &tim_mb] {
            let response = send(&ctx.app, method, uri, None, Some(cookie), Body::empty()).await;
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{method}");
        }
    }

    assert_eq!(ctx.count("pengguna").await, pengguna);
}

#[sqlx::test]
async fn both_routes_are_gone_without_an_admin_secret_on_the_server(db: PgPool) {
    let ctx = setup_with(db, None);
    ctx.mahasiswa(TARGET).await;
    ctx.posisi(5).await;

    for (method, uri) in [("DELETE", URI), ("POST", SEED_URI)] {
        let response = send(&ctx.app, method, uri, Some(SECRET), None, Body::empty()).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method}");
    }
    assert_eq!(ctx.count("mahasiswa").await, 1);
}

#[sqlx::test]
async fn never_removes_a_file_outside_the_upload_folder(db: PgPool) {
    let ctx = setup(db);
    let target = ctx.mahasiswa(TARGET).await;
    let id_posisi = ctx.posisi(5).await;
    let id = ctx.pendaftaran(&target, id_posisi, "biasa", None).await;

    let outside = ctx.root.0.join("luar.pdf");
    std::fs::write(&outside, b"%PDF").unwrap();
    sqlx::query(
        "INSERT INTO berkas_pendaftaran (id_pendaftaran, jenis_berkas, nama_berkas, ukuran_berkas)
         VALUES ($1, 'CV', '../luar.pdf', 4)",
    )
    .bind(id)
    .execute(&ctx.db)
    .await
    .unwrap();

    let response = hapus(&ctx).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["jumlah_berkas"], 2);
    assert_eq!(body["jumlah_berkas_gagal"], 1);
    assert!(outside.exists());
    assert!(!ctx.has_file("biasa.pdf"));
    assert!(!ctx.has_file("biasa-surat.pdf"));
}

#[sqlx::test]
async fn a_failed_delete_leaves_every_row_and_file_in_place(db: PgPool) {
    let ctx = setup(db);
    let target = ctx.mahasiswa(TARGET).await;
    let id_posisi = ctx.posisi(5).await;
    let id = ctx.pendaftaran(&target, id_posisi, "tetap", None).await;
    let rows = ctx.rows_of(target.id_pengguna, id).await;

    // The tim_mb row refers to the pengguna row, so the very last delete fails.
    sqlx::query("INSERT INTO tim_mb (id_pengguna) VALUES ($1)")
        .bind(target.id_pengguna)
        .execute(&ctx.db)
        .await
        .unwrap();

    let response = hapus(&ctx).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    assert_eq!(ctx.rows_of(target.id_pengguna, id).await, rows);
    assert!(ctx.has_file("tetap.pdf"));
}

#[sqlx::test]
async fn the_old_login_loses_its_access_after_deleting(db: PgPool) {
    let ctx = setup(db);
    let target = ctx.mahasiswa(TARGET).await;
    let cookie = ctx.cookie(&target);

    assert_eq!(hapus(&ctx).await.status(), StatusCode::OK);

    for uri in ["/api/v1/mahasiswa/data", "/api/v1/pendaftaran"] {
        let response = get(&ctx, &cookie, uri).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{uri}");
    }
}

#[sqlx::test]
async fn concurrent_deletes_each_count_what_they_removed(db: PgPool) {
    let ctx = setup(db);
    let target = ctx.mahasiswa(TARGET).await;
    let id_posisi = ctx.posisi(5).await;
    ctx.pendaftaran(&target, id_posisi, "a", None).await;

    let (a, b) = tokio::join!(hapus(&ctx), hapus(&ctx));

    assert_eq!(a.status(), StatusCode::OK);
    assert_eq!(b.status(), StatusCode::OK);
    let total = json_body(a).await["jumlah_mahasiswa"].as_i64().unwrap()
        + json_body(b).await["jumlah_mahasiswa"].as_i64().unwrap();
    assert_eq!(total, 1);
    assert_eq!(ctx.kuota(id_posisi).await, 5);
}

#[sqlx::test]
async fn seeding_adds_50_mahasiswa_with_one_new_pendaftaran_each(db: PgPool) {
    let ctx = setup(db);
    let id_posisi = ctx.posisi(5).await;

    let response = seed(&ctx).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = json_body(response).await;
    assert_eq!(body["jumlah_mahasiswa"], 50);
    assert_eq!(body["jumlah_pendaftaran"], 50);
    assert_eq!(body["jumlah_berkas"], ctx.count("berkas_pendaftaran").await);

    assert_eq!(ctx.count("pengguna").await, 50);
    assert_eq!(ctx.count("mahasiswa").await, 50);
    assert_eq!(ctx.count("pendaftaran").await, 50);
    assert_eq!(ctx.count("log_status").await, 50);

    let (emails, nama_berbeda, per_orang, menunggu, di_posisi): (i64, i64, i64, i64, i64) =
        sqlx::query_as(
            "SELECT count(*) FILTER (WHERE pg.email LIKE '%@student.ub.ac.id'),
                    count(DISTINCT m.nama),
                    count(DISTINCT p.id_pengguna),
                    count(*) FILTER (WHERE s.status = 'Menunggu Verifikasi'),
                    count(*) FILTER (WHERE p.id_posisi = $1)
             FROM mahasiswa m
             JOIN pengguna pg ON pg.id_pengguna = m.id_pengguna
             JOIN pendaftaran p ON p.id_pengguna = m.id_pengguna
             JOIN status_pendaftaran s ON s.id_status = p.id_status",
        )
        .bind(id_posisi)
        .fetch_one(&ctx.db)
        .await
        .unwrap();
    assert_eq!(emails, 50);
    assert_eq!(per_orang, 50);
    assert_eq!(menunggu, 50);
    assert_eq!(di_posisi, 50);
    assert!(nama_berbeda > 1, "{nama_berbeda}");
}

#[sqlx::test]
async fn every_seeded_pendaftaran_has_a_cv_a_surat_and_zero_to_three_extras(db: PgPool) {
    let ctx = setup(db);
    ctx.posisi(5).await;

    assert_eq!(seed(&ctx).await.status(), StatusCode::CREATED);

    // Per pendaftaran: the kinds of its files, in the order they were saved.
    let jenis: Vec<(i32, Vec<String>)> = sqlx::query_as(
        "SELECT id_pendaftaran, array_agg(jenis_berkas ORDER BY id_berkas)
         FROM berkas_pendaftaran
         GROUP BY id_pendaftaran",
    )
    .fetch_all(&ctx.db)
    .await
    .unwrap();
    assert_eq!(jenis.len(), 50);

    let mut per_jumlah_tambahan = [0; 4];
    for (id, jenis) in &jenis {
        assert_eq!(jenis[0], "Surat Pernyataan DPA", "{id}");
        assert_eq!(jenis[1], "CV", "{id}");
        let tambahan = &jenis[2..];
        assert!(
            tambahan
                .iter()
                .all(|j| j == "Portfolio dan Berkas Tambahan"),
            "{id}: {jenis:?}"
        );
        per_jumlah_tambahan[tambahan.len()] += 1;
    }
    // 50 spread evenly over 0, 1, 2 and 3 extra files.
    for (jumlah, mahasiswa) in per_jumlah_tambahan.iter().enumerate() {
        assert!(
            *mahasiswa == 12 || *mahasiswa == 13,
            "{jumlah}: {mahasiswa}"
        );
    }
}

#[sqlx::test]
async fn every_seeded_file_is_a_real_pdf_on_disk(db: PgPool) {
    let ctx = setup(db);
    ctx.posisi(5).await;

    assert_eq!(seed(&ctx).await.status(), StatusCode::CREATED);

    let berkas: Vec<(String, i32, i32)> = sqlx::query_as(
        "SELECT b.nama_berkas, b.ukuran_berkas, p.id_pengguna
         FROM berkas_pendaftaran b
         JOIN pendaftaran p ON p.id_pendaftaran = b.id_pendaftaran",
    )
    .fetch_all(&ctx.db)
    .await
    .unwrap();
    assert!(berkas.len() >= 100, "{}", berkas.len());

    for (nama, ukuran, id_pengguna) in &berkas {
        // Named like the files uploaded through POST /pendaftaran.
        let (pemilik, sisa) = nama.split_once('_').unwrap();
        assert_eq!(pemilik, id_pengguna.to_string(), "{nama}");
        let (slug, acak) = sisa.rsplit_once('_').unwrap();
        assert!(
            ["surat-dpa", "cv", "berkas-tambahan"].contains(&slug),
            "{nama}"
        );
        assert_eq!(
            acak.len(),
            "0123456789abcdef0123456789abcdef.pdf".len(),
            "{nama}"
        );

        let isi = std::fs::read(ctx.uploads().join(nama)).unwrap();
        assert!(isi.starts_with(b"%PDF-"), "{nama}");
        assert!(isi.ends_with(b"%%EOF\n"), "{nama}");
        assert_eq!(isi.len(), *ukuran as usize, "{nama}");
    }

    // Nothing else was written.
    let di_folder = std::fs::read_dir(ctx.uploads()).unwrap().count();
    assert_eq!(di_folder, berkas.len());
}

#[sqlx::test]
async fn a_seed_whose_files_cannot_be_written_saves_nothing(db: PgPool) {
    let ctx = setup(db);
    ctx.posisi(5).await;

    // A file where the upload folder should be, so no file can be written.
    std::fs::remove_dir(ctx.uploads()).unwrap();
    std::fs::write(ctx.uploads(), b"").unwrap();

    let response = seed(&ctx).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    assert_eq!(ctx.count("pengguna").await, 0);
    assert_eq!(ctx.count("pendaftaran").await, 0);
    assert_eq!(ctx.count("berkas_pendaftaran").await, 0);
}

#[sqlx::test]
async fn seeding_spreads_the_pendaftaran_over_every_open_position(db: PgPool) {
    let ctx = setup(db);
    let terbuka = [
        ctx.posisi(5).await,
        ctx.posisi(5).await,
        ctx.posisi(5).await,
    ];
    let tutup = ctx.posisi_pada(-3, -1).await;
    let belum = ctx.posisi_pada(1, 3).await;

    assert_eq!(seed(&ctx).await.status(), StatusCode::CREATED);

    let jumlah = |id_posisi: i32| {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pendaftaran WHERE id_posisi = $1")
            .bind(id_posisi)
            .fetch_one(&ctx.db)
    };
    for id_posisi in terbuka {
        let n = jumlah(id_posisi).await.unwrap();
        assert!(n == 16 || n == 17, "{id_posisi}: {n}");
    }
    assert_eq!(jumlah(tutup).await.unwrap(), 0);
    assert_eq!(jumlah(belum).await.unwrap(), 0);
}

#[sqlx::test]
async fn seeding_ignores_the_quota(db: PgPool) {
    let ctx = setup(db);
    let id_posisi = ctx.posisi(2).await;

    assert_eq!(seed(&ctx).await.status(), StatusCode::CREATED);

    // 50 pendaftaran on a position whose quota is 2, which is only how many will be accepted.
    assert_eq!(ctx.count("pendaftaran").await, 50);
    assert_eq!(ctx.kuota(id_posisi).await, 2);
}

#[sqlx::test]
async fn seeded_data_works_like_real_data(db: PgPool) {
    let ctx = setup(db);
    ctx.posisi(5).await;
    let tim_mb = ctx.tim_mb().await;
    let tim_mb = ctx.cookie(&tim_mb);
    assert_eq!(seed(&ctx).await.status(), StatusCode::CREATED);
    let user = ctx.seeded_user().await;
    let cookie = ctx.cookie(&user);

    // Tim MB sees all of them in the list and the detail.
    let list = json_body(get(&ctx, &tim_mb, "/api/v1/pendaftaran?r=50").await).await;
    let list = list["data"].as_array().unwrap();
    assert_eq!(list.len(), 50);
    let id = list[0]["id_pendaftaran"].as_i64().unwrap();
    assert_eq!(list[0]["status"], "Menunggu Verifikasi");
    assert_eq!(list[0]["ada_jadwal_seleksi"], false);

    let uri = format!("/api/v1/pendaftaran/{id}");
    let response = get(&ctx, &tim_mb, &uri).await;
    assert_eq!(response.status(), StatusCode::OK);
    let detail = json_body(response).await;
    assert_eq!(detail["log"].as_array().unwrap().len(), 1);
    let berkas = detail["berkas"].as_array().unwrap();
    assert!((2..=5).contains(&berkas.len()), "{}", berkas.len());

    // Every file can be downloaded, by Tim MB and by its owner.
    for item in berkas {
        let uri = format!("/api/v1/berkas/{}", item["id_berkas"]);
        let response = get(&ctx, &tim_mb, &uri).await;
        assert_eq!(response.status(), StatusCode::OK, "{item}");
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/pdf");
        let isi = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(isi.starts_with(b"%PDF-"), "{item}");
    }
    let id_berkas_pemilik: i64 = sqlx::query_scalar(
        "SELECT b.id_berkas::bigint
         FROM berkas_pendaftaran b
         JOIN pendaftaran p ON p.id_pendaftaran = b.id_pendaftaran
         WHERE p.id_pengguna = $1
         LIMIT 1",
    )
    .bind(user.id_pengguna)
    .fetch_one(&ctx.db)
    .await
    .unwrap();
    let response = get(
        &ctx,
        &cookie,
        &format!("/api/v1/berkas/{id_berkas_pemilik}"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    // Tim MB can process them.
    let response = send(
        &ctx.app,
        "PATCH",
        &format!("/api/v1/pendaftaran?id={id}"),
        None,
        Some(&tim_mb),
        Body::from(r#"{ "status": "Lolos Seleksi Administrasi" }"#),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    // The data diri is complete, and every value passes the rules of the form.
    let response = get(&ctx, &cookie, "/api/v1/mahasiswa/data").await;
    assert_eq!(response.status(), StatusCode::OK);
    let data = json_body(response).await;
    let patch = json!({
        "nim": data["nim"],
        "nama": data["nama"],
        "program_studi": data["program_studi"],
        "ipk": data["ipk"],
        "no_hp": data["no_hp"],
        "jenis_kelamin": data["jenis_kelamin"],
        "semester": data["semester"],
    });
    assert!(
        patch.as_object().unwrap().values().all(|v| !v.is_null()),
        "{patch}"
    );
    let response = send(
        &ctx.app,
        "PATCH",
        "/api/v1/mahasiswa/data",
        None,
        Some(&cookie),
        Body::from(patch.to_string()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[sqlx::test]
async fn seeding_only_uses_positions_of_a_running_period(db: PgPool) {
    let ctx = setup(db);
    let satu = ctx.posisi(5).await;
    let dua = ctx.posisi(5).await;
    let selesai = ctx.posisi_pada(-10, -5).await;
    let nanti = ctx.posisi_pada(5, 10).await;

    assert_eq!(seed(&ctx).await.status(), StatusCode::CREATED);

    let di_posisi = |id: i32| {
        let db = ctx.db.clone();
        async move {
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pendaftaran WHERE id_posisi = $1")
                .bind(id)
                .fetch_one(&db)
                .await
                .unwrap()
        }
    };
    assert_eq!(di_posisi(selesai).await, 0);
    assert_eq!(di_posisi(nanti).await, 0);
    assert_eq!(di_posisi(satu).await + di_posisi(dua).await, 50);
    assert!(di_posisi(satu).await > 0 && di_posisi(dua).await > 0);
}

#[sqlx::test]
async fn seeding_without_a_running_position_adds_nothing(db: PgPool) {
    let ctx = setup(db);

    let response = seed(&ctx).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_body(response).await["message"],
        "Tidak ada posisi di periode yang sedang berlangsung."
    );

    ctx.posisi_pada(-10, -5).await;
    assert_eq!(seed(&ctx).await.status(), StatusCode::CONFLICT);

    assert_eq!(ctx.count("pengguna").await, 0);
    assert_eq!(ctx.count("pendaftaran").await, 0);
}

#[sqlx::test]
async fn seeding_again_adds_50_more_and_deleting_removes_every_file(db: PgPool) {
    let ctx = setup(db);
    let target = ctx.mahasiswa(TARGET).await;
    ctx.posisi(5).await;

    assert_eq!(seed(&ctx).await.status(), StatusCode::CREATED);
    assert_eq!(seed(&ctx).await.status(), StatusCode::CREATED);

    assert_eq!(ctx.count("mahasiswa").await, 101);
    assert_eq!(ctx.count("pendaftaran").await, 100);
    assert_eq!(ctx.rows_of(target.id_pengguna, -1).await, 2);
    let berkas = ctx.count("berkas_pendaftaran").await;
    assert_eq!(
        std::fs::read_dir(ctx.uploads()).unwrap().count() as i64,
        berkas
    );

    let response = hapus(&ctx).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["jumlah_mahasiswa"], 101);
    assert_eq!(body["jumlah_pendaftaran"], 100);
    assert_eq!(body["jumlah_berkas"], berkas);
    assert_eq!(body["jumlah_berkas_gagal"], 0);
    assert_eq!(ctx.count("pengguna").await, 0);
    assert_eq!(std::fs::read_dir(ctx.uploads()).unwrap().count(), 0);
}
