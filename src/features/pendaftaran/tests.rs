use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, Response, StatusCode, header},
};
use serde_json::Value;
use sqlx::{AssertSqlSafe, PgPool};
use tower::ServiceExt;

use super::pembatalan;
use crate::{
    app,
    config::Config,
    email::{Email, PengirimEmail},
    features::auth::User,
    state::AppState,
};

const URI: &str = "/api/v1/pendaftaran";
const BOUNDARY: &str = "batas-pengujian";
const MAX_BERKAS: usize = 2 * 1024 * 1024;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let name = format!("gomagang-test-{:032x}", rand::random::<u128>());
        Self(std::env::temp_dir().join(name))
    }

    fn files(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.0) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Ctx {
    app: Router,
    state: AppState,
    db: PgPool,
    uploads: TempDir,
    email: Arc<Mutex<Vec<Email>>>,
}

fn app_with_upload_dir(db: PgPool, upload_dir: &Path) -> (Router, AppState) {
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
        upload_dir: upload_dir.to_path_buf(),
        surat_signing_key: None,
        smtp: None,
    };
    let mut state = AppState::new(&config, db);
    state.email = Arc::new(PengirimEmail::Uji(Arc::default()));
    (app::build(&config, state.clone()), state)
}

fn setup(db: PgPool) -> Ctx {
    let uploads = TempDir::new();
    let (app, state) = app_with_upload_dir(db.clone(), &uploads.0);
    let PengirimEmail::Uji(email) = &*state.email else {
        unreachable!()
    };
    let email = email.clone();
    Ctx {
        app,
        state,
        db,
        uploads,
        email,
    }
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

impl Ctx {
    /// Mahasiswa dengan data diri lengkap.
    async fn mahasiswa(&self, email: &str) -> (User, String) {
        let user = insert_pengguna(&self.db, email).await;
        sqlx::query(
            "INSERT INTO mahasiswa
                 (id_pengguna, nim, nama, program_studi, ipk, no_hp, jenis_kelamin, semester)
             VALUES ($1, $1::text, 'Budi', 'Teknik Informatika', 3.5, '081234567890', 'L', 6)",
        )
        .bind(user.id_pengguna)
        .execute(&self.db)
        .await
        .unwrap();
        let cookie = cookie_for(&self.state, &user);
        (user, cookie)
    }

    async fn panitia_magang(&self) -> String {
        let user = insert_pengguna(&self.db, "panitia@gmail.com").await;
        sqlx::query("INSERT INTO panitia_magang (id_pengguna) VALUES ($1)")
            .bind(user.id_pengguna)
            .execute(&self.db)
            .await
            .unwrap();
        cookie_for(&self.state, &user)
    }

    /// Posisi dengan kuota ini, di periode dari `mulai` sampai `tutup` relatif terhadap waktu
    /// sekarang dalam WIB. Periode boleh beririsan di sini, karena aturan itu milik `POST
    /// /periode`.
    async fn posisi(&self, kuota: i32, mulai: &str, tutup: &str) -> i32 {
        let id_perusahaan: i32 = sqlx::query_scalar(
            "INSERT INTO perusahaan_mitra (nama_perusahaan) VALUES ('PT Satu')
             RETURNING id_perusahaan",
        )
        .fetch_one(&self.db)
        .await
        .unwrap();
        let id_periode: i32 = sqlx::query_scalar(
            "INSERT INTO periode_pendaftaran (waktu_mulai, waktu_tutup)
             VALUES (now() AT TIME ZONE 'Asia/Jakarta' + $1::interval,
                     now() AT TIME ZONE 'Asia/Jakarta' + $2::interval)
             RETURNING id_periode",
        )
        .bind(mulai)
        .bind(tutup)
        .fetch_one(&self.db)
        .await
        .unwrap();
        sqlx::query_scalar(
            "INSERT INTO posisi_magang
                 (id_perusahaan, posisi, deskripsi_persyaratan, kuota, id_periode)
             VALUES ($1, 'Backend', 'REST API', $2, $3)
             RETURNING id_posisi",
        )
        .bind(id_perusahaan)
        .bind(kuota)
        .bind(id_periode)
        .fetch_one(&self.db)
        .await
        .unwrap()
    }

    async fn posisi_berjalan(&self, kuota: i32) -> i32 {
        self.posisi(kuota, "-1 day", "1 day").await
    }

    /// Posisi lain di periode yang sama dengan `id_posisi`.
    async fn posisi_seperiode(&self, id_posisi: i32) -> i32 {
        sqlx::query_scalar(
            "INSERT INTO posisi_magang
                 (id_perusahaan, posisi, deskripsi_persyaratan, kuota, id_periode)
             SELECT id_perusahaan, 'Frontend', 'React', kuota, id_periode
             FROM posisi_magang WHERE id_posisi = $1
             RETURNING id_posisi",
        )
        .bind(id_posisi)
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

    async fn count(&self, table: &str) -> i64 {
        sqlx::query_scalar(AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
            .fetch_one(&self.db)
            .await
            .unwrap()
    }

    async fn set_status(&self, id_pendaftaran: i64, status: &str) {
        sqlx::query(
            "UPDATE pendaftaran
             SET id_status = (SELECT id_status FROM status_pendaftaran WHERE status = $2)
             WHERE id_pendaftaran = $1",
        )
        .bind(id_pendaftaran as i32)
        .bind(status)
        .execute(&self.db)
        .await
        .unwrap();
    }

    async fn daftar(&self, cookie: &str, id_posisi: i32) -> Response<Body> {
        post(&self.app, Some(cookie), form(id_posisi)).await
    }

    async fn assert_nothing_saved(&self) {
        assert_eq!(self.count("pendaftaran").await, 0);
        assert_eq!(self.count("log_status").await, 0);
        assert_eq!(self.count("berkas_pendaftaran").await, 0);
        assert_eq!(self.uploads.files(), Vec::<String>::new());
    }
}

enum Part<'a> {
    Text(&'a str, &'a str),
    File(&'a str, &'a str, Vec<u8>),
}

fn multipart(parts: Vec<Part>) -> Vec<u8> {
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        match part {
            Part::Text(name, value) => {
                body.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}")
                        .as_bytes(),
                );
            }
            Part::File(name, filename, content) => {
                body.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n\
                         Content-Type: application/pdf\r\n\r\n"
                    )
                    .as_bytes(),
                );
                body.extend_from_slice(&content);
            }
        }
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

fn pdf(content: &str) -> Vec<u8> {
    format!("%PDF-1.4 {content}").into_bytes()
}

fn pdf_of_size(size: usize) -> Vec<u8> {
    let mut content = b"%PDF-".to_vec();
    content.resize(size, b'x');
    content
}

fn form(id_posisi: i32) -> Vec<u8> {
    let id = id_posisi.to_string();
    multipart(vec![
        Part::Text("id_posisi", &id),
        Part::File("surat_dpa", "surat.pdf", pdf("surat")),
        Part::File("cv", "cv.pdf", pdf("cv")),
    ])
}

async fn post(app: &Router, cookie: Option<&str>, body: Vec<u8>) -> Response<Body> {
    let mut request = Request::builder().method("POST").uri(URI).header(
        header::CONTENT_TYPE,
        format!("multipart/form-data; boundary={BOUNDARY}"),
    );
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    app.clone()
        .oneshot(request.body(Body::from(body)).unwrap())
        .await
        .unwrap()
}

async fn download(app: &Router, cookie: Option<&str>, id_berkas: &str) -> Response<Body> {
    let mut request = Request::builder().uri(format!("/api/v1/berkas/{id_berkas}"));
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    app.clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn json_body(response: Response<Body>) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn assert_rejected(response: Response<Body>, status: StatusCode, message: &str) {
    assert_eq!(response.status(), status, "{message}");
    assert_eq!(json_body(response).await["message"], message);
}

/// Mendaftar, lalu mengembalikan id pendaftaran dan id berkas pertamanya.
async fn daftar_ok(ctx: &Ctx, cookie: &str, id_posisi: i32) -> (i64, String) {
    let response = ctx.daftar(cookie, id_posisi).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let created = json_body(response).await;
    (
        created["id_pendaftaran"].as_i64().unwrap(),
        created["berkas"][0]["id_berkas"]
            .as_i64()
            .unwrap()
            .to_string(),
    )
}

#[sqlx::test]
async fn migration_seeds_every_status(db: PgPool) {
    let statuses: Vec<String> =
        sqlx::query_scalar("SELECT status FROM status_pendaftaran ORDER BY id_status")
            .fetch_all(&db)
            .await
            .unwrap();

    assert_eq!(
        statuses,
        [
            "Menunggu Verifikasi",
            "Lolos Seleksi Administrasi",
            "Diterima",
            "Aktif",
            "Selesai",
            "Tidak Lolos",
            "Batal/Mengundurkan Diri",
        ]
    );
}

#[sqlx::test]
async fn registering_saves_everything_and_leaves_the_quota_alone(db: PgPool) {
    let ctx = setup(db);
    let (user, cookie) = ctx.mahasiswa("m@student.ub.ac.id").await;
    let id_posisi = ctx.posisi_berjalan(5).await;

    let response = ctx.daftar(&cookie, id_posisi).await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let created = json_body(response).await;
    assert_eq!(created["id_posisi"], id_posisi);
    assert_eq!(created["status"], "Menunggu Verifikasi");
    assert!(created["created_at"].as_str().unwrap().ends_with("+07:00"));
    assert_eq!(created["created_at"], created["updated_at"]);
    assert_eq!(created.as_object().unwrap().len(), 6);

    let berkas = created["berkas"].as_array().unwrap();
    assert_eq!(berkas.len(), 2);
    assert_eq!(berkas[0]["jenis_berkas"], "Surat Pernyataan DPA");
    assert_eq!(berkas[0]["ukuran_berkas"], pdf("surat").len());
    assert_eq!(berkas[1]["jenis_berkas"], "CV");
    assert_eq!(berkas[1]["ukuran_berkas"], pdf("cv").len());

    for (file, content) in berkas.iter().zip([pdf("surat"), pdf("cv")]) {
        let nama = file["nama_berkas"].as_str().unwrap();
        assert!(
            nama.starts_with(&format!("{}_", user.id_pengguna)),
            "{nama}"
        );
        assert!(nama.ends_with(".pdf"), "{nama}");
        assert_eq!(std::fs::read(ctx.uploads.0.join(nama)).unwrap(), content);
    }

    assert_eq!(ctx.kuota(id_posisi).await, 5);

    let (id_pengguna, status_lama, id_pengubah, status_baru): (
        i32,
        Option<i32>,
        Option<i32>,
        String,
    ) = sqlx::query_as(
        "SELECT p.id_pengguna, l.status_lama, l.id_pengubah, s.status
             FROM log_status l
             JOIN pendaftaran p ON p.id_pendaftaran = l.id_pendaftaran
             JOIN status_pendaftaran s ON s.id_status = l.status_baru",
    )
    .fetch_one(&ctx.db)
    .await
    .unwrap();
    assert_eq!(id_pengguna, user.id_pengguna);
    assert_eq!(status_lama, None);
    assert_eq!(id_pengubah, None);
    assert_eq!(status_baru, "Menunggu Verifikasi");
}

#[sqlx::test]
async fn times_are_saved_in_wib_whatever_the_server_time_zone(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("m@student.ub.ac.id").await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    daftar_ok(&ctx, &cookie, id_posisi).await;

    let is_wib: bool = sqlx::query_scalar(
        "SELECT bool_and(abs(extract(epoch FROM now() AT TIME ZONE 'Asia/Jakarta' - t)) < 60)
         FROM (
             SELECT created_at AS t FROM pendaftaran
             UNION ALL SELECT updated_at FROM pendaftaran
             UNION ALL SELECT created_at FROM log_status
         ) waktu",
    )
    .fetch_one(&ctx.db)
    .await
    .unwrap();
    assert!(is_wib);
}

#[sqlx::test]
async fn ignores_the_uploaded_file_name(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("m@student.ub.ac.id").await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let id = id_posisi.to_string();

    let body = multipart(vec![
        Part::Text("id_posisi", &id),
        Part::File("surat_dpa", "../../jahat.pdf", pdf("surat")),
        Part::File("cv", "/etc/jahat.pdf", pdf("cv")),
    ]);
    let response = post(&ctx.app, Some(&cookie), body).await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let created = json_body(response).await;
    for file in created["berkas"].as_array().unwrap() {
        let nama = file["nama_berkas"].as_str().unwrap();
        assert!(!nama.contains("jahat") && !nama.contains('/'), "{nama}");
    }
    assert_eq!(ctx.uploads.files().len(), 2);
}

#[sqlx::test]
async fn accepts_files_at_exactly_the_size_limit(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("m@student.ub.ac.id").await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let id = id_posisi.to_string();

    let mut parts = vec![
        Part::Text("id_posisi", &id),
        Part::File("surat_dpa", "surat.pdf", pdf_of_size(MAX_BERKAS)),
        Part::File("cv", "cv.pdf", pdf_of_size(MAX_BERKAS)),
    ];
    // And the most extra files there may be, each as large as allowed.
    for _ in 0..5 {
        parts.push(Part::File(
            "berkas_tambahan",
            "lain.pdf",
            pdf_of_size(MAX_BERKAS),
        ));
    }
    let response = post(&ctx.app, Some(&cookie), multipart(parts)).await;

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(ctx.uploads.files().len(), 7);
}

#[sqlx::test]
async fn saves_each_extra_file_after_the_required_ones(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("m@student.ub.ac.id").await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let id = id_posisi.to_string();

    let body = multipart(vec![
        Part::Text("id_posisi", &id),
        Part::File("berkas_tambahan", "portfolio.pdf", pdf("portfolio")),
        Part::File("surat_dpa", "surat.pdf", pdf("surat")),
        Part::File("cv", "cv.pdf", pdf("cv")),
        // An empty input, as a browser sends it, is not a file.
        Part::File("berkas_tambahan", "", Vec::new()),
        Part::File("berkas_tambahan", "sertifikat.pdf", pdf("sertifikat")),
    ]);
    let response = post(&ctx.app, Some(&cookie), body).await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let created = json_body(response).await;
    let berkas = created["berkas"].as_array().unwrap();
    let jenis: Vec<&str> = berkas
        .iter()
        .map(|file| file["jenis_berkas"].as_str().unwrap())
        .collect();
    assert_eq!(
        jenis,
        [
            "Surat Pernyataan DPA",
            "CV",
            "Portfolio dan Berkas Tambahan",
            "Portfolio dan Berkas Tambahan",
        ]
    );
    for (file, isi) in berkas[2..]
        .iter()
        .zip([pdf("portfolio"), pdf("sertifikat")])
    {
        let nama = file["nama_berkas"].as_str().unwrap();
        assert!(nama.contains("_berkas-tambahan_"), "{nama}");
        assert_eq!(std::fs::read(ctx.uploads.0.join(nama)).unwrap(), isi);
    }

    // The detail lists them too, so Panitia Magang can download each one.
    let id_pendaftaran = created["id_pendaftaran"].as_i64().unwrap();
    let detail =
        json_body(get(&ctx.app, Some(&cookie), &format!("{URI}/{id_pendaftaran}")).await).await;
    assert_eq!(detail["berkas"].as_array().unwrap().len(), 4);
}

#[sqlx::test]
async fn rejects_bad_forms_without_saving(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("m@student.ub.ac.id").await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let id = id_posisi.to_string();
    let surat = || Part::File("surat_dpa", "surat.pdf", pdf("surat"));
    let cv = || Part::File("cv", "cv.pdf", pdf("cv"));

    let wajib = "Surat Pernyataan DPA dan CV wajib diunggah.";
    let terlalu_besar = "Ukuran berkas maksimal 2 MB.";
    let cases = vec![
        (vec![surat(), cv()], "Posisi wajib dipilih."),
        (
            vec![Part::Text("id_posisi", "abc"), surat(), cv()],
            "id_posisi harus berupa angka.",
        ),
        (vec![Part::Text("id_posisi", &id), cv()], wajib),
        (vec![Part::Text("id_posisi", &id), surat()], wajib),
        (
            vec![
                Part::Text("id_posisi", &id),
                Part::File("surat_dpa", "surat.pdf", Vec::new()),
                cv(),
            ],
            wajib,
        ),
        (
            vec![
                Part::Text("id_posisi", &id),
                Part::File("surat_dpa", "surat.pdf", b"bukan pdf".to_vec()),
                cv(),
            ],
            "Berkas harus berformat PDF.",
        ),
        (
            vec![
                Part::Text("id_posisi", &id),
                surat(),
                Part::File("cv", "cv.pdf", pdf_of_size(MAX_BERKAS + 1)),
            ],
            terlalu_besar,
        ),
        (
            vec![
                Part::Text("id_posisi", &id),
                surat(),
                Part::File("cv", "cv.pdf", pdf_of_size(3 * MAX_BERKAS)),
            ],
            terlalu_besar,
        ),
        (
            vec![Part::Text("id_posisi", &id), surat(), surat(), cv()],
            "Field tidak dikenal atau dikirim lebih dari sekali.",
        ),
        (
            vec![
                Part::Text("id_posisi", &id),
                surat(),
                cv(),
                Part::Text("id_status", "3"),
            ],
            "Field tidak dikenal atau dikirim lebih dari sekali.",
        ),
        (
            vec![Part::Text("id_posisi", "999999"), surat(), cv()],
            "Posisi tidak ditemukan.",
        ),
        (
            vec![
                Part::Text("id_posisi", &id),
                surat(),
                cv(),
                Part::File("berkas_tambahan", "lain.pdf", b"bukan pdf".to_vec()),
            ],
            "Berkas harus berformat PDF.",
        ),
        (
            vec![
                Part::Text("id_posisi", &id),
                surat(),
                cv(),
                Part::File("berkas_tambahan", "lain.pdf", pdf_of_size(MAX_BERKAS + 1)),
            ],
            terlalu_besar,
        ),
        (
            [Part::Text("id_posisi", &id), surat(), cv()]
                .into_iter()
                .chain((0..6).map(|_| Part::File("berkas_tambahan", "lain.pdf", pdf("lain"))))
                .collect(),
            "Berkas tambahan maksimal 5 file.",
        ),
    ];

    for (parts, message) in cases {
        let response = post(&ctx.app, Some(&cookie), multipart(parts)).await;
        assert_rejected(response, StatusCode::UNPROCESSABLE_ENTITY, message).await;
    }

    ctx.assert_nothing_saved().await;
}

#[sqlx::test]
async fn rejects_a_body_that_is_not_multipart(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("m@student.ub.ac.id").await;

    let request = Request::builder()
        .method("POST")
        .uri(URI)
        .header(header::COOKIE, &cookie)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let response = ctx.app.clone().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    ctx.assert_nothing_saved().await;
}

#[sqlx::test]
async fn rejects_incomplete_data_diri(db: PgPool) {
    let ctx = setup(db);
    let id_posisi = ctx.posisi_berjalan(5).await;

    for column in [
        "nim",
        "nama",
        "program_studi",
        "ipk",
        "no_hp",
        "jenis_kelamin",
        "semester",
    ] {
        let (user, cookie) = ctx.mahasiswa(&format!("{column}@student.ub.ac.id")).await;
        sqlx::query(AssertSqlSafe(format!(
            "UPDATE mahasiswa SET {column} = NULL WHERE id_pengguna = $1"
        )))
        .bind(user.id_pengguna)
        .execute(&ctx.db)
        .await
        .unwrap();

        let response = ctx.daftar(&cookie, id_posisi).await;
        assert_rejected(
            response,
            StatusCode::UNPROCESSABLE_ENTITY,
            "Lengkapi data diri terlebih dahulu sebelum mendaftar.",
        )
        .await;
    }

    ctx.assert_nothing_saved().await;
}

#[sqlx::test]
async fn rejects_positions_outside_a_running_period(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("m@student.ub.ac.id").await;

    for (mulai, tutup) in [("-2 days", "-1 day"), ("1 day", "2 days")] {
        let id_posisi = ctx.posisi(5, mulai, tutup).await;
        let response = ctx.daftar(&cookie, id_posisi).await;
        assert_rejected(
            response,
            StatusCode::CONFLICT,
            "Periode pendaftaran posisi ini tidak sedang berlangsung.",
        )
        .await;
    }

    ctx.assert_nothing_saved().await;
}

#[sqlx::test]
async fn more_mahasiswa_than_the_quota_can_register(db: PgPool) {
    let ctx = setup(db);
    let id_posisi = ctx.posisi_berjalan(1).await;

    // The quota is how many will be accepted, not how many may register.
    for email in [
        "a@student.ub.ac.id",
        "b@student.ub.ac.id",
        "c@student.ub.ac.id",
    ] {
        let (_, cookie) = ctx.mahasiswa(email).await;
        daftar_ok(&ctx, &cookie, id_posisi).await;
    }

    assert_eq!(ctx.count("pendaftaran").await, 3);
    assert_eq!(ctx.kuota(id_posisi).await, 1);
}

#[sqlx::test]
async fn rejects_a_second_running_registration_for_the_same_position(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("m@student.ub.ac.id").await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let message = "Kamu masih punya pendaftaran yang berjalan di posisi ini.";

    let (id_pendaftaran, _) = daftar_ok(&ctx, &cookie, id_posisi).await;
    let response = ctx.daftar(&cookie, id_posisi).await;
    assert_rejected(response, StatusCode::CONFLICT, message).await;

    // A status that is not final still counts as running.
    ctx.set_status(id_pendaftaran, "Diterima").await;
    let response = ctx.daftar(&cookie, id_posisi).await;
    assert_rejected(response, StatusCode::CONFLICT, message).await;

    // Once it has ended, the same position is open again.
    ctx.set_status(id_pendaftaran, "Tidak Lolos").await;
    daftar_ok(&ctx, &cookie, id_posisi).await;
}

#[sqlx::test]
async fn allows_two_registrations_per_period_at_most(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("m@student.ub.ac.id").await;
    let (_, other) = ctx.mahasiswa("n@student.ub.ac.id").await;
    let first_posisi = ctx.posisi_berjalan(5).await;
    let ids = [
        first_posisi,
        ctx.posisi_seperiode(first_posisi).await,
        ctx.posisi_seperiode(first_posisi).await,
    ];
    let message = "Kamu sudah mendaftar 2 kali di periode ini.";

    let (first, _) = daftar_ok(&ctx, &cookie, ids[0]).await;
    let (second, _) = daftar_ok(&ctx, &cookie, ids[1]).await;

    let response = ctx.daftar(&cookie, ids[2]).await;
    assert_rejected(response, StatusCode::CONFLICT, message).await;

    // The limit belongs to each mahasiswa.
    daftar_ok(&ctx, &other, ids[2]).await;

    // Registrations that have ended still use up the period's two chances.
    for status in ["Batal/Mengundurkan Diri", "Tidak Lolos", "Selesai"] {
        ctx.set_status(first, status).await;
        ctx.set_status(second, "Tidak Lolos").await;
        let response = ctx.daftar(&cookie, ids[2]).await;
        assert_rejected(response, StatusCode::CONFLICT, message).await;
    }

    // Hiding a position does not give its registration back.
    sqlx::query("UPDATE posisi_magang SET deleted_at = now() WHERE id_posisi = $1")
        .bind(ids[0])
        .execute(&ctx.db)
        .await
        .unwrap();
    let response = ctx.daftar(&cookie, ids[2]).await;
    assert_rejected(response, StatusCode::CONFLICT, message).await;

    // Another period starts with two new chances.
    let next_period = ctx.posisi_berjalan(5).await;
    daftar_ok(&ctx, &cookie, next_period).await;
}

#[sqlx::test]
async fn several_concurrent_mahasiswa_all_get_in_past_the_quota(db: PgPool) {
    let ctx = setup(db);
    let (_, a) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let (_, b) = ctx.mahasiswa("b@student.ub.ac.id").await;
    let (_, c) = ctx.mahasiswa("c@student.ub.ac.id").await;
    let id_posisi = ctx.posisi_berjalan(1).await;

    let (a, b, c) = tokio::join!(
        ctx.daftar(&a, id_posisi),
        ctx.daftar(&b, id_posisi),
        ctx.daftar(&c, id_posisi),
    );

    let created = [a, b, c]
        .iter()
        .filter(|response| response.status() == StatusCode::CREATED)
        .count();
    assert_eq!(created, 3);
    assert_eq!(ctx.count("pendaftaran").await, 3);
    assert_eq!(ctx.uploads.files().len(), 6);
    assert_eq!(ctx.kuota(id_posisi).await, 1);
}

#[sqlx::test]
async fn concurrent_requests_cannot_get_around_the_limits(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("m@student.ub.ac.id").await;
    let same = ctx.posisi_berjalan(5).await;

    let (a, b, c) = tokio::join!(
        ctx.daftar(&cookie, same),
        ctx.daftar(&cookie, same),
        ctx.daftar(&cookie, same),
    );
    let created = [a, b, c]
        .iter()
        .filter(|response| response.status() == StatusCode::CREATED)
        .count();
    assert_eq!(created, 1);

    let (_, other) = ctx.mahasiswa("n@student.ub.ac.id").await;
    let first = ctx.posisi_berjalan(5).await;
    let ids = [
        first,
        ctx.posisi_seperiode(first).await,
        ctx.posisi_seperiode(first).await,
    ];
    let (a, b, c) = tokio::join!(
        ctx.daftar(&other, ids[0]),
        ctx.daftar(&other, ids[1]),
        ctx.daftar(&other, ids[2]),
    );
    let created = [a, b, c]
        .iter()
        .filter(|response| response.status() == StatusCode::CREATED)
        .count();
    assert_eq!(created, 2);
}

#[sqlx::test]
async fn undoes_the_rows_when_the_files_cannot_be_written(db: PgPool) {
    let ctx = setup(db.clone());
    let (_, cookie) = ctx.mahasiswa("m@student.ub.ac.id").await;
    let id_posisi = ctx.posisi_berjalan(5).await;

    // A plain file where the folder should be.
    let blocked = TempDir::new();
    std::fs::write(&blocked.0, b"").unwrap();
    let (app, _) = app_with_upload_dir(db, &blocked.0);

    let response = post(&app, Some(&cookie), form(id_posisi)).await;
    std::fs::remove_file(&blocked.0).unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    ctx.assert_nothing_saved().await;
}

#[sqlx::test]
async fn only_mahasiswa_can_register(db: PgPool) {
    let ctx = setup(db);
    let id_posisi = ctx.posisi_berjalan(5).await;

    let response = post(&ctx.app, None, form(id_posisi)).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = post(&ctx.app, Some("access_token=bukan-jwt"), form(id_posisi)).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let panitia_magang = ctx.panitia_magang().await;
    assert_eq!(
        ctx.daftar(&panitia_magang, id_posisi).await.status(),
        StatusCode::FORBIDDEN
    );

    let user = insert_pengguna(&ctx.db, "tanpa.peran@gmail.com").await;
    let no_role = cookie_for(&ctx.state, &user);
    assert_eq!(
        ctx.daftar(&no_role, id_posisi).await.status(),
        StatusCode::FORBIDDEN
    );

    ctx.assert_nothing_saved().await;
}

#[sqlx::test]
async fn the_owner_and_panitia_magang_can_download_a_file(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("m@student.ub.ac.id").await;
    let panitia_magang = ctx.panitia_magang().await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let (_, id_berkas) = daftar_ok(&ctx, &cookie, id_posisi).await;

    for who in [&cookie, &panitia_magang] {
        let response = download(&ctx.app, Some(who), &id_berkas).await;

        assert_eq!(response.status(), StatusCode::OK);
        let headers = response.headers();
        assert_eq!(headers[header::CONTENT_TYPE], "application/pdf");
        assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(headers[header::CACHE_CONTROL], "private, no-store");
        let disposition = headers[header::CONTENT_DISPOSITION].to_str().unwrap();
        assert!(
            disposition.starts_with("attachment; filename=\""),
            "{disposition}"
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body.to_vec(), pdf("surat"));
    }
}

#[sqlx::test]
async fn nobody_else_can_download_a_file(db: PgPool) {
    let ctx = setup(db);
    let (_, owner) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let (_, other) = ctx.mahasiswa("b@student.ub.ac.id").await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let (_, id_berkas) = daftar_ok(&ctx, &owner, id_posisi).await;

    let response = download(&ctx.app, Some(&other), &id_berkas).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = download(&ctx.app, None, &id_berkas).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let user = insert_pengguna(&ctx.db, "tanpa.peran@gmail.com").await;
    let no_role = cookie_for(&ctx.state, &user);
    let response = download(&ctx.app, Some(&no_role), &id_berkas).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn downloading_something_missing_finds_nothing(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("m@student.ub.ac.id").await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let (_, id_berkas) = daftar_ok(&ctx, &cookie, id_posisi).await;

    let response = download(&ctx.app, Some(&cookie), "999999").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = download(&ctx.app, Some(&cookie), "abc").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    for file in ctx.uploads.files() {
        std::fs::remove_file(ctx.uploads.0.join(file)).unwrap();
    }
    let response = download(&ctx.app, Some(&cookie), &id_berkas).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

async fn get(app: &Router, cookie: Option<&str>, uri: &str) -> Response<Body> {
    let mut request = Request::builder().uri(uri);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    app.clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

fn ids(list: &Value) -> Vec<i64> {
    list["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id_pendaftaran"].as_i64().unwrap())
        .collect()
}

#[sqlx::test]
async fn mahasiswa_lists_only_their_own(db: PgPool) {
    let ctx = setup(db);
    let (user, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let (_, other) = ctx.mahasiswa("b@student.ub.ac.id").await;
    let (_, tanpa_daftar) = ctx.mahasiswa("c@student.ub.ac.id").await;
    let id_posisi = ctx.posisi_berjalan(5).await;

    let (mine, _) = daftar_ok(&ctx, &cookie, id_posisi).await;
    daftar_ok(&ctx, &other, id_posisi).await;

    let response = get(&ctx.app, Some(&cookie), URI).await;
    assert_eq!(response.status(), StatusCode::OK);
    let list = json_body(response).await;
    assert_eq!(ids(&list), [mine]);

    let item = &list["data"][0];
    assert_eq!(item["status"], "Menunggu Verifikasi");
    assert!(item["created_at"].as_str().unwrap().ends_with("+07:00"));
    assert!(item["updated_at"].as_str().unwrap().ends_with("+07:00"));
    assert_eq!(item["id_posisi"], id_posisi);
    assert_eq!(item["posisi"], "Backend");
    assert!(item["id_periode"].is_i64());
    assert!(item["perusahaan"]["id_perusahaan"].is_i64());
    assert_eq!(item["perusahaan"]["nama_perusahaan"], "PT Satu");
    assert_eq!(item["mahasiswa"]["id_pengguna"], user.id_pengguna);
    assert_eq!(item["mahasiswa"]["nim"], user.id_pengguna.to_string());
    assert_eq!(item["mahasiswa"]["nama"], "Budi");
    assert_eq!(item["ada_jadwal_seleksi"], false);
    assert_eq!(item["ada_plotting_dpm"], false);
    assert_eq!(item["batas_konfirmasi"], Value::Null);
    assert_eq!(item.as_object().unwrap().len(), 12);
    assert_eq!(item["mahasiswa"].as_object().unwrap().len(), 3);

    let response = get(&ctx.app, Some(&tanpa_daftar), URI).await;
    assert_eq!(json_body(response).await["data"], serde_json::json!([]));
}

#[sqlx::test]
async fn panitia_magang_lists_everyone_in_every_status_newest_first(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let id_posisi = ctx.posisi_berjalan(50).await;

    let response = get(&ctx.app, Some(&panitia_magang), URI).await;
    assert_eq!(json_body(response).await["data"], serde_json::json!([]));

    let statuses = [
        "Menunggu Verifikasi",
        "Lolos Seleksi Administrasi",
        "Diterima",
        "Aktif",
        "Selesai",
        "Tidak Lolos",
        "Batal/Mengundurkan Diri",
    ];
    let mut created = Vec::new();
    for (i, status) in statuses.iter().enumerate() {
        let (_, cookie) = ctx.mahasiswa(&format!("m{i}@student.ub.ac.id")).await;
        let (id, _) = daftar_ok(&ctx, &cookie, id_posisi).await;
        ctx.set_status(id, status).await;
        created.push(id);
    }

    // Older by id but newer by time, so the time decides.
    sqlx::query("UPDATE pendaftaran SET created_at = created_at + interval '1 hour' WHERE id_pendaftaran = $1")
        .bind(created[0] as i32)
        .execute(&ctx.db)
        .await
        .unwrap();

    let list = json_body(get(&ctx.app, Some(&panitia_magang), URI).await).await;

    let mut expected: Vec<i64> = created[1..].iter().rev().copied().collect();
    expected.insert(0, created[0]);
    assert_eq!(ids(&list), expected);

    let mut seen: Vec<&str> = list["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["status"].as_str().unwrap())
        .collect();
    seen.sort_unstable();
    let mut all = statuses;
    all.sort_unstable();
    assert_eq!(seen, all);
}

#[sqlx::test]
async fn listing_needs_a_token_and_a_known_role(db: PgPool) {
    let ctx = setup(db);

    assert_eq!(
        get(&ctx.app, None, URI).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get(&ctx.app, Some("access_token=bukan-jwt"), URI)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let user = insert_pengguna(&ctx.db, "tanpa.peran@gmail.com").await;
    let no_role = cookie_for(&ctx.state, &user);
    assert_eq!(
        get(&ctx.app, Some(&no_role), URI).await.status(),
        StatusCode::FORBIDDEN
    );
}

#[sqlx::test]
async fn detail_shows_everything_about_a_new_pendaftaran(db: PgPool) {
    let ctx = setup(db);
    let (user, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let (id, id_berkas) = daftar_ok(&ctx, &cookie, id_posisi).await;

    let response = get(&ctx.app, Some(&cookie), &format!("{URI}/{id}")).await;

    assert_eq!(response.status(), StatusCode::OK);
    let detail = json_body(response).await;
    assert_eq!(detail["id_pendaftaran"], id);
    assert_eq!(detail["status"], "Menunggu Verifikasi");
    assert_eq!(detail["id_posisi"], id_posisi);
    assert_eq!(detail["posisi"], "Backend");
    assert_eq!(detail["perusahaan"]["nama_perusahaan"], "PT Satu");

    let mahasiswa = &detail["mahasiswa"];
    assert_eq!(mahasiswa["id_pengguna"], user.id_pengguna);
    assert_eq!(mahasiswa["nim"], user.id_pengguna.to_string());
    assert_eq!(mahasiswa["email"], "a@student.ub.ac.id");
    assert_eq!(mahasiswa["nama"], "Budi");
    assert_eq!(mahasiswa["program_studi"], "Teknik Informatika");
    assert_eq!(mahasiswa["ipk"], 3.5);
    assert_eq!(mahasiswa["no_hp"], "081234567890");
    assert_eq!(mahasiswa["jenis_kelamin"], "L");
    assert_eq!(mahasiswa["semester"], 6);

    let berkas = detail["berkas"].as_array().unwrap();
    assert_eq!(berkas.len(), 2);
    assert_eq!(berkas[0]["id_berkas"].to_string(), id_berkas);
    assert_eq!(berkas[0]["jenis_berkas"], "Surat Pernyataan DPA");
    assert_eq!(berkas[1]["jenis_berkas"], "CV");

    let log = detail["log"].as_array().unwrap();
    assert_eq!(log.len(), 1);
    assert!(log[0]["id_log"].is_i64());
    assert_eq!(log[0]["status_lama"], Value::Null);
    assert_eq!(log[0]["status_baru"], "Menunggu Verifikasi");
    assert!(log[0]["created_at"].as_str().unwrap().ends_with("+07:00"));
    assert!(log[0].get("id_pengubah").is_none());

    assert_eq!(detail["jadwal_seleksi"], Value::Null);
    assert_eq!(detail["surat_keterangan"], Value::Null);
    assert_eq!(detail["plotting_dpm"], Value::Null);
}

#[sqlx::test]
async fn detail_shows_the_later_stages(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let (id, _) = daftar_ok(&ctx, &cookie, id_posisi).await;
    let id = id as i32;

    for sql in [
        "INSERT INTO jadwal_seleksi_substansi (id_pendaftaran, tanggal, jam, lokasi)
         VALUES ($1, '2026-11-05', '13:30', 'Daring')",
        "INSERT INTO surat_keterangan
             (id_pendaftaran, nama_berkas, ukuran_berkas, nomor_surat, isi, tanda_tangan,
              hash_berkas)
         VALUES ($1, 'surat.pdf', 1234, '00001/SKM/MB-FILKOM/2026', '{}', 'tanda-tangan',
                 repeat('0', 64))",
        "INSERT INTO plotting_dpm (id_pendaftaran, nama_dpm, kontak_dpm)
         VALUES ($1, 'Dr. Rina', 'rina@ub.ac.id')",
        "INSERT INTO log_status (id_pendaftaran, status_lama, status_baru, created_at)
         SELECT $1, lama.id_status, baru.id_status, now() AT TIME ZONE 'Asia/Jakarta' + interval '1 hour'
         FROM status_pendaftaran lama, status_pendaftaran baru
         WHERE lama.status = 'Menunggu Verifikasi' AND baru.status = 'Diterima'",
    ] {
        sqlx::query(sql).bind(id).execute(&ctx.db).await.unwrap();
    }
    ctx.set_status(id.into(), "Diterima").await;

    let response = get(&ctx.app, Some(&cookie), &format!("{URI}/{id}")).await;
    let detail = json_body(response).await;

    assert_eq!(detail["status"], "Diterima");
    assert_eq!(detail["jadwal_seleksi"]["tanggal"], "2026-11-05");
    assert_eq!(detail["jadwal_seleksi"]["jam"], "13:30");
    assert_eq!(detail["jadwal_seleksi"]["lokasi"], "Daring");
    assert_eq!(detail["surat_keterangan"]["nama_berkas"], "surat.pdf");
    assert_eq!(detail["surat_keterangan"]["ukuran_berkas"], 1234);
    assert_eq!(
        detail["surat_keterangan"]["nomor_surat"],
        "00001/SKM/MB-FILKOM/2026"
    );
    assert_eq!(detail["plotting_dpm"]["nama_dpm"], "Dr. Rina");
    assert_eq!(detail["plotting_dpm"]["kontak_dpm"], "rina@ub.ac.id");

    let log = detail["log"].as_array().unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0]["status_baru"], "Menunggu Verifikasi");
    assert_eq!(log[1]["status_lama"], "Menunggu Verifikasi");
    assert_eq!(log[1]["status_baru"], "Diterima");
}

#[sqlx::test]
async fn detail_is_only_for_the_owner_and_panitia_magang(db: PgPool) {
    let ctx = setup(db);
    let (owner, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let (_, other) = ctx.mahasiswa("b@student.ub.ac.id").await;
    let panitia_magang = ctx.panitia_magang().await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let (id, _) = daftar_ok(&ctx, &cookie, id_posisi).await;
    let uri = format!("{URI}/{id}");

    let response = get(&ctx.app, Some(&panitia_magang), &uri).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await["mahasiswa"]["id_pengguna"],
        owner.id_pengguna
    );

    let response = get(&ctx.app, Some(&other), &uri).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .is_empty()
    );

    assert_eq!(
        get(&ctx.app, None, &uri).await.status(),
        StatusCode::UNAUTHORIZED
    );

    let user = insert_pengguna(&ctx.db, "tanpa.peran@gmail.com").await;
    let no_role = cookie_for(&ctx.state, &user);
    assert_eq!(
        get(&ctx.app, Some(&no_role), &uri).await.status(),
        StatusCode::FORBIDDEN
    );
}

#[sqlx::test]
async fn detail_of_something_missing_finds_nothing(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let panitia_magang = ctx.panitia_magang().await;

    for who in [&cookie, &panitia_magang] {
        let response = get(&ctx.app, Some(who), &format!("{URI}/999999")).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let response = get(&ctx.app, Some(who), &format!("{URI}/abc")).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[sqlx::test]
async fn stopping_the_period_closes_registration_and_keeps_what_was_sent(db: PgPool) {
    let ctx = setup(db);
    let (_, first) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let (_, second) = ctx.mahasiswa("b@student.ub.ac.id").await;
    let panitia_magang = ctx.panitia_magang().await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let (id_pendaftaran, _) = daftar_ok(&ctx, &first, id_posisi).await;

    let id_periode: i32 =
        sqlx::query_scalar("SELECT id_periode FROM posisi_magang WHERE id_posisi = $1")
            .bind(id_posisi)
            .fetch_one(&ctx.db)
            .await
            .unwrap();
    let request = Request::builder()
        .method("PATCH")
        .uri(format!("/api/v1/periode/{id_periode}/stop"))
        .header(header::COOKIE, &panitia_magang)
        .body(Body::empty())
        .unwrap();
    let response = ctx.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = ctx.daftar(&second, id_posisi).await;
    assert_rejected(
        response,
        StatusCode::CONFLICT,
        "Periode pendaftaran posisi ini tidak sedang berlangsung.",
    )
    .await;

    let response = get(&ctx.app, Some(&first), URI).await;
    assert_eq!(ids(&json_body(response).await), [id_pendaftaran]);
}

const STATUSES: [&str; 7] = [
    "Menunggu Verifikasi",
    "Lolos Seleksi Administrasi",
    "Diterima",
    "Aktif",
    "Selesai",
    "Tidak Lolos",
    "Batal/Mengundurkan Diri",
];
const ALLOWED: [(&str, &str); 5] = [
    ("Menunggu Verifikasi", "Lolos Seleksi Administrasi"),
    ("Menunggu Verifikasi", "Tidak Lolos"),
    ("Lolos Seleksi Administrasi", "Diterima"),
    ("Lolos Seleksi Administrasi", "Tidak Lolos"),
    ("Aktif", "Selesai"),
];

async fn patch_status(
    app: &Router,
    cookie: Option<&str>,
    query: &str,
    body: &str,
) -> Response<Body> {
    let mut request = Request::builder()
        .method("PATCH")
        .uri(format!("{URI}{query}"))
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    app.clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

async fn ubah_status(app: &Router, cookie: &str, id: i64, status: &str) -> Response<Body> {
    let body = serde_json::json!({ "status": status }).to_string();
    patch_status(app, Some(cookie), &format!("?id={id}"), &body).await
}

impl Ctx {
    fn email_terkirim(&self) -> Vec<Email> {
        self.email.lock().unwrap().clone()
    }

    /// Status, `updated_at`, dan jumlah baris log, untuk memastikan tidak ada yang berubah.
    async fn snapshot(&self, id: i64) -> (String, String, i64) {
        sqlx::query_as(
            "SELECT s.status, p.updated_at::text,
                    (SELECT count(*) FROM log_status l WHERE l.id_pendaftaran = p.id_pendaftaran)
             FROM pendaftaran p
             JOIN status_pendaftaran s ON s.id_status = p.id_status
             WHERE p.id_pendaftaran = $1",
        )
        .bind(id as i32)
        .fetch_one(&self.db)
        .await
        .unwrap()
    }
}

#[sqlx::test]
async fn panitia_magang_changes_a_status_and_it_is_logged(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let panitia_magang = ctx.panitia_magang().await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let (id, _) = daftar_ok(&ctx, &cookie, id_posisi).await;
    let id_panitia_magang: i32 = sqlx::query_scalar("SELECT id_pengguna FROM panitia_magang")
        .fetch_one(&ctx.db)
        .await
        .unwrap();

    let response = ubah_status(&ctx.app, &panitia_magang, id, "Lolos Seleksi Administrasi").await;

    assert_eq!(response.status(), StatusCode::OK);
    let detail = json_body(response).await;
    assert_eq!(detail["id_pendaftaran"], id);
    assert_eq!(detail["status"], "Lolos Seleksi Administrasi");
    assert_eq!(detail["mahasiswa"]["email"], "a@student.ub.ac.id");
    assert_eq!(detail["berkas"].as_array().unwrap().len(), 2);

    let log = detail["log"].as_array().unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[1]["status_lama"], "Menunggu Verifikasi");
    assert_eq!(log[1]["status_baru"], "Lolos Seleksi Administrasi");
    assert_eq!(log[1]["created_at"], detail["updated_at"]);

    // The owner sees the same thing through the detail endpoint.
    let response = get(&ctx.app, Some(&cookie), &format!("{URI}/{id}")).await;
    assert_eq!(json_body(response).await, detail);

    // The test database runs in UTC, so this also shows the times were saved in WIB.
    let (id_pengubah, is_wib): (Option<i32>, bool) = sqlx::query_as(
        "SELECT l.id_pengubah,
                abs(extract(epoch FROM now() AT TIME ZONE 'Asia/Jakarta' - l.created_at)) < 60
                AND l.created_at = p.updated_at
         FROM log_status l
         JOIN pendaftaran p ON p.id_pendaftaran = l.id_pendaftaran
         WHERE l.status_lama IS NOT NULL",
    )
    .fetch_one(&ctx.db)
    .await
    .unwrap();
    assert_eq!(id_pengubah, Some(id_panitia_magang));
    assert!(is_wib);
}

#[sqlx::test]
async fn a_status_change_emails_the_mahasiswa(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let panitia_magang = ctx.panitia_magang().await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let (id, _) = daftar_ok(&ctx, &cookie, id_posisi).await;
    assert!(ctx.email_terkirim().is_empty());

    let response = ubah_status(&ctx.app, &panitia_magang, id, "Lolos Seleksi Administrasi").await;
    assert_eq!(response.status(), StatusCode::OK);
    let detail = json_body(response).await;

    let terkirim = ctx.email_terkirim();
    assert_eq!(terkirim.len(), 1);
    let email = &terkirim[0];
    assert_eq!(email.kepada, "a@student.ub.ac.id");
    assert_eq!(
        email.subjek,
        "Status pendaftaran magang: Lolos Seleksi Administrasi"
    );
    assert!(email.isi.starts_with("Halo Budi,"), "{}", email.isi);
    for bagian in [
        format!("Posisi      : {}", detail["posisi"].as_str().unwrap()),
        format!(
            "Perusahaan  : {}",
            detail["perusahaan"]["nama_perusahaan"].as_str().unwrap()
        ),
        "Status lama : Menunggu Verifikasi".to_string(),
        "Status baru : Lolos Seleksi Administrasi".to_string(),
        "http://frontend.test".to_string(),
    ] {
        assert!(
            email.isi.contains(&bagian),
            "{bagian} missing in {}",
            email.isi
        );
    }

    let html = email.html.as_ref().unwrap();
    let cid: Vec<_> = html.gambar.iter().map(|gambar| gambar.cid).collect();
    assert_eq!(cid, ["logo", "ikon-berhasil"]);
    for bagian in [
        "cid:logo",
        "cid:ikon-berhasil",
        "Kamu lolos seleksi administrasi",
        "Halo Budi,",
        &format!("href=\"http://frontend.test/dasbor/{id}\""),
    ] {
        assert!(
            html.isi.contains(bagian),
            "{bagian} missing in {}",
            html.isi
        );
    }
    assert!(!html.isi.contains("{{"));
}

#[sqlx::test]
async fn a_rejection_email_shows_a_red_mark_and_escapes_the_data(db: PgPool) {
    let ctx = setup(db);
    let (user, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    sqlx::query("UPDATE mahasiswa SET nama = '<b>Budi</b> & Ani' WHERE id_pengguna = $1")
        .bind(user.id_pengguna)
        .execute(&ctx.db)
        .await
        .unwrap();
    let panitia_magang = ctx.panitia_magang().await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let (id, _) = daftar_ok(&ctx, &cookie, id_posisi).await;

    let response = ubah_status(&ctx.app, &panitia_magang, id, "Tidak Lolos").await;
    assert_eq!(response.status(), StatusCode::OK);

    let terkirim = ctx.email_terkirim();
    let html = terkirim[0].html.as_ref().unwrap();
    let cid: Vec<_> = html.gambar.iter().map(|gambar| gambar.cid).collect();
    assert_eq!(cid, ["logo", "ikon-gagal"]);
    assert!(html.isi.contains("cid:ikon-gagal"));
    assert!(html.isi.contains("Halo &lt;b&gt;Budi&lt;/b&gt; &amp; Ani,"));
    assert!(!html.isi.contains("<b>Budi</b>"));
}

#[sqlx::test]
async fn a_rejected_status_change_sends_no_email(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let panitia_magang = ctx.panitia_magang().await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let (id, _) = daftar_ok(&ctx, &cookie, id_posisi).await;

    let response = ubah_status(&ctx.app, &panitia_magang, id, "Selesai").await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let response = ubah_status(&ctx.app, &panitia_magang, id + 1000, "Tidak Lolos").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    assert!(ctx.email_terkirim().is_empty());
}

#[sqlx::test]
async fn only_the_allowed_changes_go_through(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let panitia_magang = ctx.panitia_magang().await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let (id, _) = daftar_ok(&ctx, &cookie, id_posisi).await;

    for lama in STATUSES {
        for baru in STATUSES {
            ctx.set_status(id, lama).await;
            let before = ctx.snapshot(id).await;

            let response = ubah_status(&ctx.app, &panitia_magang, id, baru).await;

            if ALLOWED.contains(&(lama, baru)) {
                assert_eq!(response.status(), StatusCode::OK, "{lama} -> {baru}");
                assert_eq!(json_body(response).await["status"], baru);
            } else {
                assert_rejected(
                    response,
                    StatusCode::CONFLICT,
                    "Perubahan status ini tidak diizinkan.",
                )
                .await;
                assert_eq!(ctx.snapshot(id).await, before, "{lama} -> {baru}");
            }
        }
    }
}

#[sqlx::test]
async fn a_status_change_leaves_the_quota_alone(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let panitia_magang = ctx.panitia_magang().await;
    let id_posisi = ctx.posisi_berjalan(1).await;
    let (id, _) = daftar_ok(&ctx, &cookie, id_posisi).await;

    let response = ubah_status(&ctx.app, &panitia_magang, id, "Tidak Lolos").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(ctx.kuota(id_posisi).await, 1);

    // The rejected mahasiswa may try again.
    daftar_ok(&ctx, &cookie, id_posisi).await;
    assert_eq!(ctx.kuota(id_posisi).await, 1);
}

#[sqlx::test]
async fn a_status_can_change_after_the_period_has_ended(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let panitia_magang = ctx.panitia_magang().await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let (id, _) = daftar_ok(&ctx, &cookie, id_posisi).await;
    sqlx::query(
        "UPDATE periode_pendaftaran
         SET waktu_mulai = waktu_mulai - interval '30 days',
             waktu_tutup = waktu_mulai - interval '20 days'",
    )
    .execute(&ctx.db)
    .await
    .unwrap();

    let response = ubah_status(&ctx.app, &panitia_magang, id, "Lolos Seleksi Administrasi").await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[sqlx::test]
async fn rejects_bad_status_bodies_without_changing_anything(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let panitia_magang = ctx.panitia_magang().await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let (id, _) = daftar_ok(&ctx, &cookie, id_posisi).await;
    let before = ctx.snapshot(id).await;
    let query = format!("?id={id}");

    for status in ["Lulus", "", "diterima", " Diterima"] {
        let response = ubah_status(&ctx.app, &panitia_magang, id, status).await;
        assert_rejected(
            response,
            StatusCode::UNPROCESSABLE_ENTITY,
            "Status tidak dikenal.",
        )
        .await;
    }

    for body in [
        r#"{}"#,
        r#"{ "status": null }"#,
        r#"{ "status": 2 }"#,
        r#"{ "id_status": 2 }"#,
        r#"{ "status": "Diterima", "id_status": 2 }"#,
    ] {
        let response = patch_status(&ctx.app, Some(&panitia_magang), &query, body).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}"
        );
    }

    let response = patch_status(&ctx.app, Some(&panitia_magang), &query, "bukan json").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let request = Request::builder()
        .method("PATCH")
        .uri(format!("{URI}{query}"))
        .header(header::COOKIE, &panitia_magang)
        .body(Body::from(r#"{ "status": "Tidak Lolos" }"#))
        .unwrap();
    let response = ctx.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    assert_eq!(ctx.snapshot(id).await, before);
}

#[sqlx::test]
async fn changing_a_status_needs_a_valid_id(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let body = r#"{ "status": "Tidak Lolos" }"#;

    for query in ["", "?id=abc", "?id="] {
        let response = patch_status(&ctx.app, Some(&panitia_magang), query, body).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{query}");
    }

    let response = patch_status(&ctx.app, Some(&panitia_magang), "?id=999999", body).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn only_panitia_magang_can_change_a_status(db: PgPool) {
    let ctx = setup(db);
    let (_, owner) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let (_, other) = ctx.mahasiswa("b@student.ub.ac.id").await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let (id, _) = daftar_ok(&ctx, &owner, id_posisi).await;
    let before = ctx.snapshot(id).await;
    let query = format!("?id={id}");
    let body = r#"{ "status": "Tidak Lolos" }"#;

    let response = patch_status(&ctx.app, None, &query, body).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = patch_status(&ctx.app, Some("access_token=bukan-jwt"), &query, body).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let user = insert_pengguna(&ctx.db, "tanpa.peran@gmail.com").await;
    let no_role = cookie_for(&ctx.state, &user);
    for cookie in [&owner, &other, &no_role] {
        let response = patch_status(&ctx.app, Some(cookie), &query, body).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    assert_eq!(ctx.snapshot(id).await, before);
}

#[sqlx::test]
async fn only_one_of_several_concurrent_changes_wins(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let panitia_magang = ctx.panitia_magang().await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let (id, _) = daftar_ok(&ctx, &cookie, id_posisi).await;

    let (a, b, c, d) = tokio::join!(
        ubah_status(&ctx.app, &panitia_magang, id, "Lolos Seleksi Administrasi"),
        ubah_status(&ctx.app, &panitia_magang, id, "Lolos Seleksi Administrasi"),
        ubah_status(&ctx.app, &panitia_magang, id, "Tidak Lolos"),
        ubah_status(&ctx.app, &panitia_magang, id, "Tidak Lolos"),
    );

    let statuses = [a.status(), b.status(), c.status(), d.status()];
    assert!(
        statuses
            .iter()
            .all(|s| [StatusCode::OK, StatusCode::CONFLICT].contains(s)),
        "{statuses:?}"
    );
    let ok = statuses.iter().filter(|s| **s == StatusCode::OK).count();
    assert!(ok <= 2, "{statuses:?}");

    let (_, _, log) = ctx.snapshot(id).await;
    assert_eq!(log as usize, ok + 1);

    // Each log row picks up where the one before it ended.
    let chain: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM (
             SELECT status_lama,
                    lag(status_baru) OVER (ORDER BY created_at, id_log) AS sebelumnya
             FROM log_status
         ) l
         WHERE status_lama IS DISTINCT FROM sebelumnya",
    )
    .fetch_one(&ctx.db)
    .await
    .unwrap();
    assert_eq!(chain, 0);
}

async fn accept(app: &Router, cookie: Option<&str>, query: &str) -> Response<Body> {
    let mut request = Request::builder()
        .method("PATCH")
        .uri(format!("{URI}/accept{query}"));
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    app.clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn terima(app: &Router, cookie: &str, id: i64) -> Response<Body> {
    accept(app, Some(cookie), &format!("?id={id}")).await
}

const SUDAH_AKTIF: &str = "Kamu sudah memiliki pendaftaran yang aktif.";
const BUKAN_DITERIMA: &str = "Hanya pendaftaran berstatus Diterima yang bisa dikonfirmasi.";
const BATAL: &str = "Batal/Mengundurkan Diri";

impl Ctx {
    /// Mendaftar ke posisi baru, lalu langsung memindahkan pendaftarannya ke status ini.
    async fn pendaftaran(&self, cookie: &str, status: &str) -> i64 {
        let id_posisi = self.posisi_berjalan(5).await;
        let (id, _) = daftar_ok(self, cookie, id_posisi).await;
        self.set_status(id, status).await;
        id
    }

    async fn status_of(&self, id: i64) -> String {
        self.snapshot(id).await.0
    }

    /// `(status_lama, status_baru, id_pengubah)` dari baris log terakhir.
    async fn last_log(&self, id: i64) -> (Option<String>, String, Option<i32>) {
        sqlx::query_as(
            "SELECT lama.status, baru.status, l.id_pengubah
             FROM log_status l
             LEFT JOIN status_pendaftaran lama ON lama.id_status = l.status_lama
             JOIN status_pendaftaran baru ON baru.id_status = l.status_baru
             WHERE l.id_pendaftaran = $1
             ORDER BY l.created_at DESC, l.id_log DESC
             LIMIT 1",
        )
        .bind(id as i32)
        .fetch_one(&self.db)
        .await
        .unwrap()
    }
}

const KEDALUWARSA: &str =
    "Batas waktu konfirmasi 1 x 12 jam sudah lewat, sehingga pendaftaran ini dibatalkan.";

impl Ctx {
    /// Pendaftaran yang baru saja diubah Panitia Magang menjadi `Diterima`.
    async fn diterima(&self, cookie: &str, panitia_magang: &str) -> i64 {
        let id = self.pendaftaran(cookie, "Lolos Seleksi Administrasi").await;
        let response = ubah_status(&self.app, panitia_magang, id, "Diterima").await;
        assert_eq!(response.status(), StatusCode::OK);
        id
    }

    /// Memundurkan semua waktu pendaftaran ini, seolah-olah statusnya menjadi `Diterima` selama itu
    /// yang lalu.
    async fn mundurkan(&self, id: i64, selang: &str) {
        sqlx::query(
            "WITH log AS (
                 UPDATE log_status SET created_at = created_at - $2::interval
                 WHERE id_pendaftaran = $1
             )
             UPDATE pendaftaran SET updated_at = updated_at - $2::interval
             WHERE id_pendaftaran = $1",
        )
        .bind(id as i32)
        .bind(selang)
        .execute(&self.db)
        .await
        .unwrap();
    }
}

#[sqlx::test]
async fn diterima_gets_a_12_hour_deadline_and_a_firm_warning_by_email(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let panitia_magang = ctx.panitia_magang().await;
    let id = ctx.diterima(&cookie, &panitia_magang).await;

    let response = get(&ctx.app, Some(&cookie), &format!("{URI}/{id}")).await;
    let detail = json_body(response).await;
    let diharapkan: String = sqlx::query_scalar(
        "SELECT to_char(created_at + interval '12 hours', 'YYYY-MM-DD\"T\"HH24:MI:SS+07:00')
         FROM log_status WHERE id_pendaftaran = $1 AND status_lama IS NOT NULL",
    )
    .bind(id as i32)
    .fetch_one(&ctx.db)
    .await
    .unwrap();
    assert_eq!(detail["batas_konfirmasi"], diharapkan);

    let terkirim = ctx.email_terkirim();
    let email = terkirim.last().unwrap();
    assert_eq!(
        email.subjek,
        "Status pendaftaran magang: Diterima (konfirmasi dalam 1 x 12 jam)"
    );
    assert!(
        email
            .isi
            .contains("PENTING: Konfirmasi penerimaan wajib dilakukan paling lambat"),
        "{}",
        email.isi
    );
    assert!(
        email
            .isi
            .contains("otomatis dinyatakan Batal/Mengundurkan Diri")
    );
    let html = &email.html.as_ref().unwrap().isi;
    assert!(html.contains("PENTING: Konfirmasi dalam 1 x 12 jam"));
    assert!(html.contains("<strong>Batal/Mengundurkan Diri</strong>"));
    assert!(html.contains(">Konfirmasi penerimaan</a>"));
    assert!(html.contains(" WIB</strong>"));

    // Once confirmed, there is no deadline any more.
    assert_eq!(terima(&ctx.app, &cookie, id).await.status(), StatusCode::OK);
    let response = get(&ctx.app, Some(&cookie), &format!("{URI}/{id}")).await;
    assert_eq!(json_body(response).await["batas_konfirmasi"], Value::Null);
}

#[sqlx::test]
async fn a_confirmation_after_12_hours_is_refused(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;

    let (_, tepat_waktu) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let id = ctx.diterima(&tepat_waktu, &panitia_magang).await;
    ctx.mundurkan(id, "11 hours 59 minutes").await;
    assert_eq!(
        terima(&ctx.app, &tepat_waktu, id).await.status(),
        StatusCode::OK
    );

    let (_, terlambat) = ctx.mahasiswa("b@student.ub.ac.id").await;
    let id = ctx.diterima(&terlambat, &panitia_magang).await;
    ctx.mundurkan(id, "12 hours 1 minute").await;
    let before = ctx.snapshot(id).await;

    let response = terima(&ctx.app, &terlambat, id).await;

    assert_rejected(response, StatusCode::CONFLICT, KEDALUWARSA).await;
    assert_eq!(ctx.snapshot(id).await, before);
}

#[sqlx::test]
async fn unconfirmed_acceptances_are_cancelled_after_12_hours(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;

    let (_, a) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let lewat = ctx.diterima(&a, &panitia_magang).await;
    ctx.mundurkan(lewat, "12 hours").await;

    let (_, b) = ctx.mahasiswa("b@student.ub.ac.id").await;
    let belum = ctx.diterima(&b, &panitia_magang).await;
    ctx.mundurkan(belum, "11 hours 58 minutes").await;

    // Set straight to Diterima without a log row, so updated_at stands in.
    let (_, c) = ctx.mahasiswa("c@student.ub.ac.id").await;
    let tanpa_log = ctx.pendaftaran(&c, "Diterima").await;
    ctx.mundurkan(tanpa_log, "13 hours").await;

    // Other statuses are left alone, however old.
    let (_, d) = ctx.mahasiswa("d@student.ub.ac.id").await;
    let lolos = ctx.pendaftaran(&d, "Lolos Seleksi Administrasi").await;
    ctx.mundurkan(lolos, "30 days").await;

    ctx.email.lock().unwrap().clear();
    pembatalan::batalkan_yang_kedaluwarsa(&ctx.state).await;

    assert_eq!(ctx.status_of(lewat).await, BATAL);
    assert_eq!(
        ctx.last_log(lewat).await,
        (Some("Diterima".to_string()), BATAL.to_string(), None)
    );
    assert_eq!(ctx.status_of(tanpa_log).await, BATAL);
    assert_eq!(ctx.status_of(belum).await, "Diterima");
    assert_eq!(ctx.status_of(lolos).await, "Lolos Seleksi Administrasi");

    let mut terkirim = ctx.email_terkirim();
    terkirim.sort_by(|x, y| x.kepada.cmp(&y.kepada));
    let kepada: Vec<_> = terkirim.iter().map(|email| email.kepada.as_str()).collect();
    assert_eq!(kepada, ["a@student.ub.ac.id", "c@student.ub.ac.id"]);
    let email = &terkirim[0];
    assert_eq!(email.subjek, format!("Status pendaftaran magang: {BATAL}"));
    assert!(
        email
            .isi
            .contains("dibatalkan otomatis karena penerimaan tidak dikonfirmasi")
    );
    assert!(email.isi.contains("Status lama : Diterima"));
    let html = email.html.as_ref().unwrap();
    assert!(html.isi.contains("cid:ikon-gagal"));
    assert!(!html.isi.contains("PENTING"));

    // A cancelled one cannot be confirmed any more, and nothing happens on the next round.
    assert_rejected(
        terima(&ctx.app, &a, lewat).await,
        StatusCode::CONFLICT,
        BUKAN_DITERIMA,
    )
    .await;
    ctx.email.lock().unwrap().clear();
    pembatalan::batalkan_yang_kedaluwarsa(&ctx.state).await;
    assert!(ctx.email_terkirim().is_empty());
    assert_eq!(ctx.status_of(belum).await, "Diterima");
}

#[sqlx::test]
async fn accepting_activates_it_and_withdraws_the_other_running_ones(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;

    // Three registrations that have already ended, which must stay as they are. The running ones
    // come last, as a mahasiswa can only have two of those at a time.
    let selesai = ctx.pendaftaran(&cookie, "Selesai").await;
    let tidak_lolos = ctx.pendaftaran(&cookie, "Tidak Lolos").await;
    let batal = ctx.pendaftaran(&cookie, BATAL).await;
    let ended = [
        ctx.snapshot(selesai).await,
        ctx.snapshot(tidak_lolos).await,
        ctx.snapshot(batal).await,
    ];

    let target_posisi = ctx.posisi_berjalan(5).await;
    let (target, _) = daftar_ok(&ctx, &cookie, target_posisi).await;
    ctx.set_status(target, "Diterima").await;
    let other_posisi = ctx.posisi_berjalan(5).await;
    let (other, _) = daftar_ok(&ctx, &cookie, other_posisi).await;

    // Someone else's pendaftaran is not touched.
    let (_, someone) = ctx.mahasiswa("b@student.ub.ac.id").await;
    let (theirs, _) = daftar_ok(&ctx, &someone, other_posisi).await;
    let theirs_before = ctx.snapshot(theirs).await;

    let response = terima(&ctx.app, &cookie, target).await;
    assert_eq!(response.status(), StatusCode::OK);
    let detail = json_body(response).await;
    assert_eq!(detail["id_pendaftaran"], target);
    assert_eq!(detail["status"], "Aktif");
    let log = detail["log"].as_array().unwrap();
    assert_eq!(log.last().unwrap()["status_baru"], "Aktif");
    assert_eq!(log.last().unwrap()["status_lama"], "Diterima");

    assert_eq!(ctx.status_of(target).await, "Aktif");
    assert_eq!(
        ctx.last_log(target).await,
        (Some("Diterima".to_string()), "Aktif".to_string(), None)
    );

    assert_eq!(ctx.status_of(other).await, BATAL);
    assert_eq!(
        ctx.last_log(other).await,
        (
            Some("Menunggu Verifikasi".to_string()),
            BATAL.to_string(),
            None
        )
    );

    assert_eq!(
        [
            ctx.snapshot(selesai).await,
            ctx.snapshot(tidak_lolos).await,
            ctx.snapshot(batal).await,
        ],
        ended
    );
    assert_eq!(ctx.snapshot(theirs).await, theirs_before);
}

#[sqlx::test]
async fn accepting_withdraws_from_every_running_status(db: PgPool) {
    for other_status in [
        "Menunggu Verifikasi",
        "Lolos Seleksi Administrasi",
        "Diterima",
    ] {
        let ctx = setup(db.clone());
        let email = format!("{}@student.ub.ac.id", other_status.replace(' ', "."));
        let (_, cookie) = ctx.mahasiswa(&email).await;
        let target = ctx.pendaftaran(&cookie, "Diterima").await;
        let other = ctx.pendaftaran(&cookie, other_status).await;

        let response = terima(&ctx.app, &cookie, target).await;
        assert_eq!(response.status(), StatusCode::OK, "{other_status}");
        assert_eq!(ctx.status_of(target).await, "Aktif", "{other_status}");
        assert_eq!(ctx.status_of(other).await, BATAL, "{other_status}");
    }
}

#[sqlx::test]
async fn accepting_works_with_nothing_else_to_withdraw(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let target = ctx.pendaftaran(&cookie, "Diterima").await;

    let response = terima(&ctx.app, &cookie, target).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(ctx.status_of(target).await, "Aktif");
    assert_eq!(ctx.count("log_status").await, 2);
}

#[sqlx::test]
async fn only_a_diterima_pendaftaran_can_be_accepted(db: PgPool) {
    for status in STATUSES.into_iter().filter(|s| *s != "Diterima") {
        let ctx = setup(db.clone());
        let email = format!("{}@student.ub.ac.id", status.replace(['/', ' '], "."));
        let (_, cookie) = ctx.mahasiswa(&email).await;
        let target = ctx.pendaftaran(&cookie, status).await;
        let before = ctx.snapshot(target).await;

        let response = terima(&ctx.app, &cookie, target).await;
        assert_rejected(response, StatusCode::CONFLICT, BUKAN_DITERIMA).await;
        assert_eq!(ctx.snapshot(target).await, before, "{status}");
    }
}

#[sqlx::test]
async fn a_rejected_accept_withdraws_nothing(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let target = ctx.pendaftaran(&cookie, "Lolos Seleksi Administrasi").await;
    let other = ctx.pendaftaran(&cookie, "Diterima").await;
    let before = ctx.snapshot(other).await;

    let response = terima(&ctx.app, &cookie, target).await;
    assert_rejected(response, StatusCode::CONFLICT, BUKAN_DITERIMA).await;
    assert_eq!(ctx.snapshot(other).await, before);
}

#[sqlx::test]
async fn accepting_again_is_rejected_without_a_second_log(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let target = ctx.pendaftaran(&cookie, "Diterima").await;

    assert_eq!(
        terima(&ctx.app, &cookie, target).await.status(),
        StatusCode::OK
    );
    let before = ctx.snapshot(target).await;

    let response = terima(&ctx.app, &cookie, target).await;
    assert_rejected(response, StatusCode::CONFLICT, BUKAN_DITERIMA).await;
    assert_eq!(ctx.snapshot(target).await, before);
}

#[sqlx::test]
async fn cannot_accept_while_another_pendaftaran_is_aktif(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let target = ctx.pendaftaran(&cookie, "Diterima").await;
    let aktif = ctx.pendaftaran(&cookie, "Aktif").await;
    let before = (ctx.snapshot(aktif).await, ctx.snapshot(target).await);

    let response = terima(&ctx.app, &cookie, target).await;
    assert_rejected(response, StatusCode::CONFLICT, SUDAH_AKTIF).await;
    assert_eq!(
        (ctx.snapshot(aktif).await, ctx.snapshot(target).await),
        before
    );
}

#[sqlx::test]
async fn cannot_register_while_aktif_until_it_ends(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let aktif = ctx.pendaftaran(&cookie, "Aktif").await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let pendaftaran = ctx.count("pendaftaran").await;

    let response = ctx.daftar(&cookie, id_posisi).await;
    assert_rejected(response, StatusCode::CONFLICT, SUDAH_AKTIF).await;
    assert_eq!(ctx.count("pendaftaran").await, pendaftaran);

    ctx.set_status(aktif, "Selesai").await;
    assert_eq!(
        ctx.daftar(&cookie, id_posisi).await.status(),
        StatusCode::CREATED
    );
}

#[sqlx::test]
async fn a_new_place_can_be_accepted_after_the_last_one_ended(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let selesai = ctx.pendaftaran(&cookie, "Selesai").await;
    let target = ctx.pendaftaran(&cookie, "Diterima").await;

    let response = terima(&ctx.app, &cookie, target).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(ctx.status_of(target).await, "Aktif");
    assert_eq!(ctx.status_of(selesai).await, "Selesai");
}

#[sqlx::test]
async fn a_place_can_be_accepted_after_the_period_has_ended(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let target = ctx.pendaftaran(&cookie, "Diterima").await;
    sqlx::query(
        "UPDATE periode_pendaftaran
         SET waktu_mulai = waktu_mulai - interval '30 days',
             waktu_tutup = waktu_mulai - interval '29 days'",
    )
    .execute(&ctx.db)
    .await
    .unwrap();

    let response = terima(&ctx.app, &cookie, target).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[sqlx::test]
async fn the_whole_flow_runs_from_registering_to_selesai(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let panitia_magang = ctx.panitia_magang().await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let (id, _) = daftar_ok(&ctx, &cookie, id_posisi).await;

    for status in ["Lolos Seleksi Administrasi", "Diterima"] {
        let response = ubah_status(&ctx.app, &panitia_magang, id, status).await;
        assert_eq!(response.status(), StatusCode::OK, "{status}");
    }
    assert_eq!(terima(&ctx.app, &cookie, id).await.status(), StatusCode::OK);
    let response = ubah_status(&ctx.app, &panitia_magang, id, "Selesai").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(ctx.status_of(id).await, "Selesai");
}

#[sqlx::test]
async fn only_the_owner_can_accept(db: PgPool) {
    let ctx = setup(db);
    let (_, owner) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let (_, other) = ctx.mahasiswa("b@student.ub.ac.id").await;
    let panitia_magang = ctx.panitia_magang().await;
    let user = insert_pengguna(&ctx.db, "tanpa.peran@gmail.com").await;
    let no_role = cookie_for(&ctx.state, &user);
    let target = ctx.pendaftaran(&owner, "Diterima").await;
    let before = ctx.snapshot(target).await;
    let query = format!("?id={target}");

    let response = accept(&ctx.app, None, &query).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = accept(&ctx.app, Some("access_token=bukan-jwt"), &query).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    for cookie in [&other, &panitia_magang, &no_role] {
        let response = accept(&ctx.app, Some(cookie), &query).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    assert_eq!(ctx.snapshot(target).await, before);
}

#[sqlx::test]
async fn accepting_needs_a_valid_id_of_something_that_exists(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;

    for query in ["", "?id=abc", "?id=", "?id=99999999999"] {
        let response = accept(&ctx.app, Some(&cookie), query).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{query}");
    }

    let response = terima(&ctx.app, &cookie, 999_999).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn only_one_of_two_concurrent_accepts_wins(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let first = ctx.pendaftaran(&cookie, "Diterima").await;
    let second = ctx.pendaftaran(&cookie, "Diterima").await;

    let (a, b) = tokio::join!(
        terima(&ctx.app, &cookie, first),
        terima(&ctx.app, &cookie, second),
    );

    let mut statuses = [a.status(), b.status()];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::OK, StatusCode::CONFLICT]);

    let mut hasil = [ctx.status_of(first).await, ctx.status_of(second).await];
    hasil.sort();
    assert_eq!(hasil, ["Aktif".to_string(), BATAL.to_string()]);
}

#[sqlx::test]
async fn a_registration_racing_an_accept_cannot_stay_running(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let target = ctx.pendaftaran(&cookie, "Diterima").await;
    let id_posisi = ctx.posisi_berjalan(5).await;

    let (accepted, registered) = tokio::join!(
        terima(&ctx.app, &cookie, target),
        ctx.daftar(&cookie, id_posisi),
    );

    assert_eq!(accepted.status(), StatusCode::OK);
    assert!(
        [StatusCode::CREATED, StatusCode::CONFLICT].contains(&registered.status()),
        "{}",
        registered.status()
    );
    assert_eq!(ctx.status_of(target).await, "Aktif");

    let still_running: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM pendaftaran p
         JOIN status_pendaftaran s ON s.id_status = p.id_status
         WHERE s.status NOT IN ('Aktif', 'Selesai', 'Tidak Lolos', 'Batal/Mengundurkan Diri')",
    )
    .fetch_one(&ctx.db)
    .await
    .unwrap();
    assert_eq!(still_running, 0);
}

#[sqlx::test]
async fn a_panitia_magang_change_racing_an_accept_keeps_the_log_in_order(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let panitia_magang = ctx.panitia_magang().await;
    let target = ctx.pendaftaran(&cookie, "Diterima").await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let (other, _) = daftar_ok(&ctx, &cookie, id_posisi).await;

    let (accepted, _) = tokio::join!(
        terima(&ctx.app, &cookie, target),
        ubah_status(&ctx.app, &panitia_magang, other, "Tidak Lolos"),
    );
    assert_eq!(accepted.status(), StatusCode::OK);

    // Whichever came first, the other one saw its result.
    let status = ctx.status_of(other).await;
    assert!(status == BATAL || status == "Tidak Lolos", "{status}");
    let (_, _, log) = ctx.snapshot(other).await;
    assert_eq!(log, 2);
}

const JADWAL_URI: &str = "/api/v1/pendaftaran/jadwal-seleksi-substansi";
const LOLOS: &str = "Lolos Seleksi Administrasi";
const BUKAN_LOLOS: &str = "Semua pendaftaran harus berstatus Lolos Seleksi Administrasi.";

async fn put_jadwal_raw(
    app: &Router,
    cookie: Option<&str>,
    content_type: Option<&str>,
    body: String,
) -> Response<Body> {
    let mut request = Request::builder().method("PUT").uri(JADWAL_URI);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    if let Some(content_type) = content_type {
        request = request.header(header::CONTENT_TYPE, content_type);
    }
    app.clone()
        .oneshot(request.body(Body::from(body)).unwrap())
        .await
        .unwrap()
}

async fn put_jadwal(app: &Router, cookie: Option<&str>, body: Value) -> Response<Body> {
    put_jadwal_raw(app, cookie, Some("application/json"), body.to_string()).await
}

fn jadwal_body(ids: &[i64]) -> Value {
    serde_json::json!({
        "id_pendaftaran": ids,
        "tanggal": "2099-12-01",
        "jam": "13:30",
        "lokasi": "Gedung F, Ruang F2.3",
    })
}

impl Ctx {
    /// Pendaftaran milik mahasiswa baru, karena setiap mahasiswa hanya boleh punya dua pendaftaran
    /// berjalan.
    async fn pendaftaran_baru(&self, nama: &str, status: &str) -> (i64, String) {
        let (_, cookie) = self.mahasiswa(&format!("{nama}@student.ub.ac.id")).await;
        let id = self.pendaftaran(&cookie, status).await;
        (id, cookie)
    }

    async fn jadwal_of(&self, id: i64) -> Option<(String, String, String)> {
        sqlx::query_as(
            "SELECT to_char(tanggal, 'YYYY-MM-DD'), to_char(jam, 'HH24:MI'), lokasi
             FROM jadwal_seleksi_substansi
             WHERE id_pendaftaran = $1",
        )
        .bind(id as i32)
        .fetch_optional(&self.db)
        .await
        .unwrap()
    }

    /// Tanggal dalam WIB, sekian hari dari hari ini.
    async fn tanggal(&self, days: i32) -> String {
        sqlx::query_scalar(
            "SELECT to_char(
                 (now() AT TIME ZONE 'Asia/Jakarta')::date + $1::int,
                 'YYYY-MM-DD'
             )",
        )
        .bind(days)
        .fetch_one(&self.db)
        .await
        .unwrap()
    }
}

fn jadwal_awal() -> Option<(String, String, String)> {
    Some((
        "2099-12-01".to_string(),
        "13:30".to_string(),
        "Gedung F, Ruang F2.3".to_string(),
    ))
}

#[sqlx::test]
async fn saves_the_same_jadwal_on_every_pendaftaran_listed(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, cookie_a) = ctx.pendaftaran_baru("a", LOLOS).await;
    let (b, _) = ctx.pendaftaran_baru("b", LOLOS).await;
    let (c, _) = ctx.pendaftaran_baru("c", LOLOS).await;
    let (bukan, _) = ctx.pendaftaran_baru("d", LOLOS).await;
    let before = ctx.snapshot(a).await;

    let response = put_jadwal(&ctx.app, Some(&panitia_magang), jadwal_body(&[c, a, b])).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await,
        serde_json::json!({
            "id_pendaftaran": [a, b, c],
            "tanggal": "2099-12-01",
            "jam": "13:30",
            "lokasi": "Gedung F, Ruang F2.3",
        })
    );

    for id in [a, b, c] {
        assert_eq!(ctx.jadwal_of(id).await, jadwal_awal());
    }
    assert_eq!(ctx.jadwal_of(bukan).await, None);

    // The owner and Panitia Magang both see it in the detail.
    for cookie in [&cookie_a, &panitia_magang] {
        let response = get(&ctx.app, Some(cookie), &format!("{URI}/{a}")).await;
        let detail = json_body(response).await;
        assert_eq!(
            detail["jadwal_seleksi"],
            serde_json::json!({
                "tanggal": "2099-12-01",
                "jam": "13:30",
                "lokasi": "Gedung F, Ruang F2.3",
            })
        );
    }

    // The status and its log stay, while updated_at moves on.
    let after = ctx.snapshot(a).await;
    assert_eq!(after.0, before.0);
    assert_eq!(after.2, before.2);
    assert_ne!(after.1, before.1);
}

#[sqlx::test]
async fn saving_again_replaces_the_jadwal(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (lama, _) = ctx.pendaftaran_baru("a", LOLOS).await;
    let (baru, _) = ctx.pendaftaran_baru("b", LOLOS).await;

    let response = put_jadwal(&ctx.app, Some(&panitia_magang), jadwal_body(&[lama])).await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = serde_json::json!({
        "id_pendaftaran": [lama, baru],
        "tanggal": "2099-12-02",
        "jam": "09:00",
        "lokasi": "Daring (Zoom)",
    });
    let response = put_jadwal(&ctx.app, Some(&panitia_magang), body).await;
    assert_eq!(response.status(), StatusCode::OK);

    let expected = Some((
        "2099-12-02".to_string(),
        "09:00".to_string(),
        "Daring (Zoom)".to_string(),
    ));
    assert_eq!(ctx.jadwal_of(lama).await, expected);
    assert_eq!(ctx.jadwal_of(baru).await, expected);
    assert_eq!(ctx.count("jadwal_seleksi_substansi").await, 2);
}

#[sqlx::test]
async fn an_id_listed_twice_counts_once(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, _) = ctx.pendaftaran_baru("a", LOLOS).await;

    let response = put_jadwal(&ctx.app, Some(&panitia_magang), jadwal_body(&[a, a, a])).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await["id_pendaftaran"],
        serde_json::json!([a])
    );
    assert_eq!(ctx.count("jadwal_seleksi_substansi").await, 1);
}

#[sqlx::test]
async fn one_pendaftaran_in_another_status_stops_them_all(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;

    for status in STATUSES.into_iter().filter(|s| *s != LOLOS) {
        let nama = status.replace(['/', ' '], ".");
        let (punya_jadwal, _) = ctx.pendaftaran_baru(&format!("{nama}.a"), LOLOS).await;
        let (lolos, _) = ctx.pendaftaran_baru(&format!("{nama}.b"), LOLOS).await;
        let (lain, _) = ctx.pendaftaran_baru(&format!("{nama}.c"), status).await;

        let response = put_jadwal(
            &ctx.app,
            Some(&panitia_magang),
            jadwal_body(&[punya_jadwal]),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let before = ctx.snapshot(punya_jadwal).await;

        let body = serde_json::json!({
            "id_pendaftaran": [punya_jadwal, lolos, lain],
            "tanggal": "2099-12-02",
            "jam": "09:00",
            "lokasi": "Daring (Zoom)",
        });
        let response = put_jadwal(&ctx.app, Some(&panitia_magang), body).await;
        assert_rejected(response, StatusCode::CONFLICT, BUKAN_LOLOS).await;

        assert_eq!(ctx.jadwal_of(punya_jadwal).await, jadwal_awal(), "{status}");
        assert_eq!(ctx.jadwal_of(lolos).await, None, "{status}");
        assert_eq!(ctx.jadwal_of(lain).await, None, "{status}");
        assert_eq!(ctx.snapshot(punya_jadwal).await, before, "{status}");
    }
}

#[sqlx::test]
async fn one_missing_pendaftaran_stops_them_all(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, _) = ctx.pendaftaran_baru("a", LOLOS).await;

    let response = put_jadwal(&ctx.app, Some(&panitia_magang), jadwal_body(&[a, 999_999])).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(ctx.jadwal_of(a).await, None);
}

#[sqlx::test]
async fn rejects_bad_jadwal_without_saving(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, _) = ctx.pendaftaran_baru("a", LOLOS).await;
    let before = ctx.snapshot(a).await;
    let kemarin = ctx.tanggal(-1).await;
    let tanggal_salah = "Tanggal harus berformat YYYY-MM-DD.";
    let jam_salah = "Jam harus berformat HH:MM.";
    let lokasi_salah = "Lokasi harus berisi 1-255 karakter.";

    let cases = [
        (
            "id_pendaftaran",
            serde_json::json!([]),
            "Pilih minimal satu pendaftaran.",
        ),
        ("tanggal", serde_json::json!("01-12-2099"), tanggal_salah),
        ("tanggal", serde_json::json!("2099-02-30"), tanggal_salah),
        (
            "tanggal",
            serde_json::json!("2099-12-01T10:00"),
            tanggal_salah,
        ),
        ("tanggal", serde_json::json!(""), tanggal_salah),
        (
            "tanggal",
            serde_json::json!(kemarin),
            "Tanggal tidak boleh sudah lewat.",
        ),
        ("jam", serde_json::json!("24:00"), jam_salah),
        ("jam", serde_json::json!("13:60"), jam_salah),
        ("jam", serde_json::json!("13:30:00"), jam_salah),
        ("jam", serde_json::json!("1:30"), jam_salah),
        ("jam", serde_json::json!(""), jam_salah),
        ("lokasi", serde_json::json!(""), lokasi_salah),
        ("lokasi", serde_json::json!("   "), lokasi_salah),
        ("lokasi", serde_json::json!("x".repeat(256)), lokasi_salah),
    ];
    for (field, value, message) in cases {
        let mut body = jadwal_body(&[a]);
        body[field] = value;
        let response = put_jadwal(&ctx.app, Some(&panitia_magang), body).await;
        assert_rejected(response, StatusCode::UNPROCESSABLE_ENTITY, message).await;
    }

    // The framework's own rejections, which have no message.
    for field in ["id_pendaftaran", "tanggal", "jam", "lokasi"] {
        let mut body = jadwal_body(&[a]);
        body.as_object_mut().unwrap().remove(field);
        let response = put_jadwal(&ctx.app, Some(&panitia_magang), body).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{field}"
        );
    }
    let mut extra = jadwal_body(&[a]);
    extra["status"] = serde_json::json!("Diterima");
    let bad_ids = serde_json::json!({
        "id_pendaftaran": "1",
        "tanggal": "2099-12-01",
        "jam": "13:30",
        "lokasi": "Malang",
    });
    for body in [extra, bad_ids] {
        let response = put_jadwal(&ctx.app, Some(&panitia_magang), body).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    let response = put_jadwal_raw(
        &ctx.app,
        Some(&panitia_magang),
        Some("application/json"),
        "{".to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = jadwal_body(&[a]).to_string();
    let response = put_jadwal_raw(&ctx.app, Some(&panitia_magang), None, body).await;
    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    assert_eq!(ctx.jadwal_of(a).await, None);
    assert_eq!(ctx.snapshot(a).await, before);
}

#[sqlx::test]
async fn accepts_today_and_the_limits_of_each_field(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, _) = ctx.pendaftaran_baru("a", LOLOS).await;
    let hari_ini = ctx.tanggal(0).await;
    let lokasi = "é".repeat(255);

    for jam in ["00:00", "23:59"] {
        let body = serde_json::json!({
            "id_pendaftaran": [a],
            "tanggal": hari_ini,
            "jam": jam,
            "lokasi": format!("  {lokasi}  "),
        });
        let response = put_jadwal(&ctx.app, Some(&panitia_magang), body).await;
        assert_eq!(response.status(), StatusCode::OK, "{jam}");
        assert_eq!(
            ctx.jadwal_of(a).await,
            Some((hari_ini.clone(), jam.to_string(), lokasi.clone()))
        );
    }
}

#[sqlx::test]
async fn only_panitia_magang_can_save_a_jadwal(db: PgPool) {
    let ctx = setup(db);
    let (a, owner) = ctx.pendaftaran_baru("a", LOLOS).await;
    let (_, other) = ctx.mahasiswa("b@student.ub.ac.id").await;
    let user = insert_pengguna(&ctx.db, "tanpa.peran@gmail.com").await;
    let no_role = cookie_for(&ctx.state, &user);

    let response = put_jadwal(&ctx.app, None, jadwal_body(&[a])).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let bad = Some("access_token=bukan-jwt");
    let response = put_jadwal(&ctx.app, bad, jadwal_body(&[a])).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    for cookie in [&owner, &other, &no_role] {
        let response = put_jadwal(&ctx.app, Some(cookie), jadwal_body(&[a])).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    assert_eq!(ctx.jadwal_of(a).await, None);
}

#[sqlx::test]
async fn overlapping_saves_end_with_one_jadwal_for_all(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, _) = ctx.pendaftaran_baru("a", LOLOS).await;
    let (b, _) = ctx.pendaftaran_baru("b", LOLOS).await;

    let isi = |ids: [i64; 2], lokasi: &str| {
        serde_json::json!({
            "id_pendaftaran": ids,
            "tanggal": "2099-12-01",
            "jam": "13:30",
            "lokasi": lokasi,
        })
    };
    let (first, second) = tokio::join!(
        put_jadwal(&ctx.app, Some(&panitia_magang), isi([a, b], "Satu")),
        put_jadwal(&ctx.app, Some(&panitia_magang), isi([b, a], "Dua")),
    );

    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(ctx.jadwal_of(a).await, ctx.jadwal_of(b).await);
    assert_eq!(ctx.count("jadwal_seleksi_substansi").await, 2);
}

#[sqlx::test]
async fn a_status_change_racing_a_save_cannot_leave_a_jadwal_on_a_rejected_one(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, _) = ctx.pendaftaran_baru("a", LOLOS).await;

    let (saved, changed) = tokio::join!(
        put_jadwal(&ctx.app, Some(&panitia_magang), jadwal_body(&[a])),
        ubah_status(&ctx.app, &panitia_magang, a, "Tidak Lolos"),
    );

    assert_eq!(changed.status(), StatusCode::OK);
    assert_eq!(ctx.status_of(a).await, "Tidak Lolos");
    // The save went first and is kept as history, or came second and was turned away.
    match saved.status() {
        StatusCode::OK => assert_eq!(ctx.jadwal_of(a).await, jadwal_awal()),
        StatusCode::CONFLICT => assert_eq!(ctx.jadwal_of(a).await, None),
        other => panic!("{other}"),
    }
}

/// `(ada_jadwal_seleksi, ada_plotting_dpm)` setiap pendaftaran di daftar, berdasarkan id.
async fn flags_in_list(ctx: &Ctx, cookie: &str, id: i64) -> (Value, Value) {
    let list = json_body(get(&ctx.app, Some(cookie), URI).await).await;
    let item = list["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id_pendaftaran"] == id)
        .unwrap();
    (
        item["ada_jadwal_seleksi"].clone(),
        item["ada_plotting_dpm"].clone(),
    )
}

async fn insert_plotting(db: &PgPool, id: i64) {
    sqlx::query(
        "INSERT INTO plotting_dpm (id_pendaftaran, nama_dpm, kontak_dpm)
         VALUES ($1, 'Pak Dosen', '0812')",
    )
    .bind(id as i32)
    .execute(db)
    .await
    .unwrap();
}

#[sqlx::test]
async fn the_list_says_whether_jadwal_and_plotting_are_filled_in(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, owner) = ctx.pendaftaran_baru("a", LOLOS).await;
    let (other, _) = ctx.pendaftaran_baru("b", LOLOS).await;
    let (no, yes) = (Value::Bool(false), Value::Bool(true));

    for cookie in [&owner, &panitia_magang] {
        assert_eq!(
            flags_in_list(&ctx, cookie, a).await,
            (no.clone(), no.clone())
        );
    }

    let response = put_jadwal(&ctx.app, Some(&panitia_magang), jadwal_body(&[a])).await;
    assert_eq!(response.status(), StatusCode::OK);
    for cookie in [&owner, &panitia_magang] {
        assert_eq!(
            flags_in_list(&ctx, cookie, a).await,
            (yes.clone(), no.clone())
        );
    }

    insert_plotting(&ctx.db, a).await;
    for cookie in [&owner, &panitia_magang] {
        assert_eq!(
            flags_in_list(&ctx, cookie, a).await,
            (yes.clone(), yes.clone())
        );
    }

    // Each pendaftaran answers for itself.
    assert_eq!(
        flags_in_list(&ctx, &panitia_magang, other).await,
        (no.clone(), no.clone())
    );
}

#[sqlx::test]
async fn the_list_flags_each_field_on_its_own(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, _) = ctx.pendaftaran_baru("a", "Aktif").await;

    insert_plotting(&ctx.db, a).await;

    assert_eq!(
        flags_in_list(&ctx, &panitia_magang, a).await,
        (Value::Bool(false), Value::Bool(true))
    );
}

#[sqlx::test]
async fn the_list_keeps_the_flags_after_the_status_moves_on(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, _) = ctx.pendaftaran_baru("a", LOLOS).await;

    let response = put_jadwal(&ctx.app, Some(&panitia_magang), jadwal_body(&[a])).await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = ubah_status(&ctx.app, &panitia_magang, a, "Diterima").await;
    assert_eq!(response.status(), StatusCode::OK);

    assert_eq!(
        flags_in_list(&ctx, &panitia_magang, a).await,
        (Value::Bool(true), Value::Bool(false))
    );
}

#[sqlx::test]
async fn the_detail_does_not_repeat_the_flags(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, _) = ctx.pendaftaran_baru("a", LOLOS).await;

    let response = get(&ctx.app, Some(&panitia_magang), &format!("{URI}/{a}")).await;
    let detail = json_body(response).await;
    assert!(detail.get("ada_jadwal_seleksi").is_none());
    assert!(detail.get("ada_plotting_dpm").is_none());
    assert!(detail["jadwal_seleksi"].is_null());
    assert!(detail["plotting_dpm"].is_null());
}

/// Menunggu sampai ada query aplikasi yang tertahan oleh kunci baris yang dipegang test.
async fn wait_until_blocked(db: &PgPool) {
    for _ in 0..500 {
        let blocked: i64 = sqlx::query_scalar(
            "SELECT count(*)
             FROM pg_stat_activity
             WHERE datname = current_database() AND wait_event_type = 'Lock'",
        )
        .fetch_one(db)
        .await
        .unwrap();
        if blocked > 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("nothing got blocked");
}

impl Ctx {
    /// Menjalankan request selagi transaksi lain sedang memindahkan pendaftaran ini ke `status`,
    /// lalu meng-commit transaksi itu begitu request sedang menunggunya.
    async fn while_status_changes_to(
        &self,
        id: i64,
        status: &str,
        request: impl Future<Output = Response<Body>>,
    ) -> Response<Body> {
        let mut tx = self.db.begin().await.unwrap();
        sqlx::query(
            "UPDATE pendaftaran
             SET id_status = (SELECT id_status FROM status_pendaftaran WHERE status = $2)
             WHERE id_pendaftaran = $1",
        )
        .bind(id as i32)
        .bind(status)
        .execute(&mut *tx)
        .await
        .unwrap();

        let (response, _) = tokio::join!(request, async {
            wait_until_blocked(&self.db).await;
            tx.commit().await.unwrap();
        });
        response
    }
}

#[sqlx::test]
async fn saving_a_jadwal_sees_a_status_changed_while_it_waited(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, _) = ctx.pendaftaran_baru("a", LOLOS).await;

    let request = put_jadwal(&ctx.app, Some(&panitia_magang), jadwal_body(&[a]));
    let response = ctx.while_status_changes_to(a, "Tidak Lolos", request).await;

    assert_rejected(response, StatusCode::CONFLICT, BUKAN_LOLOS).await;
    assert_eq!(ctx.jadwal_of(a).await, None);
}

#[sqlx::test]
async fn saving_a_jadwal_accepts_a_status_that_became_valid_while_it_waited(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, _) = ctx.pendaftaran_baru("a", "Menunggu Verifikasi").await;

    let request = put_jadwal(&ctx.app, Some(&panitia_magang), jadwal_body(&[a]));
    let response = ctx.while_status_changes_to(a, LOLOS, request).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(ctx.jadwal_of(a).await, jadwal_awal());
}

#[sqlx::test]
async fn changing_a_status_checks_against_the_one_set_while_it_waited(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;

    // Allowed from the status set meanwhile, though not from the one it started with.
    let (a, _) = ctx.pendaftaran_baru("a", "Menunggu Verifikasi").await;
    let request = ubah_status(&ctx.app, &panitia_magang, a, "Diterima");
    let response = ctx.while_status_changes_to(a, LOLOS, request).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(ctx.status_of(a).await, "Diterima");
    assert_eq!(
        ctx.last_log(a).await.0,
        Some(LOLOS.to_string()),
        "the log starts from the status set meanwhile"
    );

    // Not allowed from the status set meanwhile, though allowed from the one it started with.
    let (b, _) = ctx.pendaftaran_baru("b", "Menunggu Verifikasi").await;
    let request = ubah_status(&ctx.app, &panitia_magang, b, LOLOS);
    let response = ctx.while_status_changes_to(b, "Tidak Lolos", request).await;
    assert_rejected(
        response,
        StatusCode::CONFLICT,
        "Perubahan status ini tidak diizinkan.",
    )
    .await;
    assert_eq!(ctx.status_of(b).await, "Tidak Lolos");
}

#[sqlx::test]
async fn accepting_withdraws_a_pendaftaran_from_the_status_set_while_it_waited(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let target = ctx.pendaftaran(&cookie, "Diterima").await;
    let other = ctx.pendaftaran(&cookie, "Menunggu Verifikasi").await;

    let request = terima(&ctx.app, &cookie, target);
    let response = ctx.while_status_changes_to(other, LOLOS, request).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(ctx.status_of(target).await, "Aktif");
    assert_eq!(ctx.status_of(other).await, BATAL);
    assert_eq!(ctx.last_log(other).await.0, Some(LOLOS.to_string()));
}

#[sqlx::test]
async fn accepting_leaves_a_pendaftaran_that_ended_while_it_waited(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("a@student.ub.ac.id").await;
    let target = ctx.pendaftaran(&cookie, "Diterima").await;
    let other = ctx.pendaftaran(&cookie, "Menunggu Verifikasi").await;

    let request = terima(&ctx.app, &cookie, target);
    let response = ctx
        .while_status_changes_to(other, "Tidak Lolos", request)
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(ctx.status_of(target).await, "Aktif");
    assert_eq!(ctx.status_of(other).await, "Tidak Lolos");
}

impl Ctx {
    /// Sebanyak `jumlah` pendaftaran milik mahasiswa baru, dari yang paling lama, dengan waktu yang
    /// jelas berbeda.
    async fn banyak_pendaftaran(&self, jumlah: i32) -> Vec<i64> {
        let mut created = Vec::new();
        for i in 0..jumlah {
            let (id, _) = self.pendaftaran_baru(&format!("hal{i}"), LOLOS).await;
            sqlx::query(
                "UPDATE pendaftaran
                 SET created_at = created_at + make_interval(mins => $2)
                 WHERE id_pendaftaran = $1",
            )
            .bind(id as i32)
            .bind(i)
            .execute(&self.db)
            .await
            .unwrap();
            created.push(id);
        }
        created
    }
}

fn info_halaman(list: &Value) -> (i64, i64, i64, i64) {
    (
        list["page"].as_i64().unwrap(),
        list["rows"].as_i64().unwrap(),
        list["total"].as_i64().unwrap(),
        list["total_pages"].as_i64().unwrap(),
    )
}

async fn halaman(ctx: &Ctx, cookie: &str, query: &str) -> Value {
    let response = get(&ctx.app, Some(cookie), &format!("{URI}{query}")).await;
    assert_eq!(response.status(), StatusCode::OK, "{query}");
    json_body(response).await
}

#[sqlx::test]
async fn pages_split_the_list_newest_first_without_overlap(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let created = ctx.banyak_pendaftaran(5).await;
    let newest_first: Vec<i64> = created.iter().rev().copied().collect();

    let first = halaman(&ctx, &panitia_magang, "?p=1&r=2").await;
    assert_eq!(ids(&first), newest_first[0..2]);
    assert_eq!(info_halaman(&first), (1, 2, 5, 3));

    let second = halaman(&ctx, &panitia_magang, "?p=2&r=2").await;
    assert_eq!(ids(&second), newest_first[2..4]);
    assert_eq!(info_halaman(&second), (2, 2, 5, 3));

    // The last page holds what is left.
    let last = halaman(&ctx, &panitia_magang, "?p=3&r=2").await;
    assert_eq!(ids(&last), newest_first[4..5]);
    assert_eq!(info_halaman(&last), (3, 2, 5, 3));
}

#[sqlx::test]
async fn total_pages_is_rounded_up_only_when_needed(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    ctx.banyak_pendaftaran(4).await;

    for (rows, pages) in [(1, 4), (2, 2), (3, 2), (4, 1), (5, 1), (100, 1)] {
        let list = halaman(&ctx, &panitia_magang, &format!("?p=1&r={rows}")).await;
        assert_eq!(list["total_pages"], pages, "{rows}");
        assert_eq!(list["total"], 4, "{rows}");
    }
}

#[sqlx::test]
async fn a_page_out_of_range_is_empty_but_still_says_the_totals(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    ctx.banyak_pendaftaran(3).await;

    let list = halaman(&ctx, &panitia_magang, "?p=9&r=2").await;
    assert_eq!(list["data"], serde_json::json!([]));
    assert_eq!(info_halaman(&list), (9, 2, 3, 2));

    // Large values cannot overflow the offset.
    let list = halaman(&ctx, &panitia_magang, "?p=2147483647&r=2147483647").await;
    assert_eq!(list["data"], serde_json::json!([]));
    assert_eq!(list["total"], 3);
}

#[sqlx::test]
async fn without_p_and_r_it_is_page_1_of_25_rows(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;

    let empty = halaman(&ctx, &panitia_magang, "").await;
    assert_eq!(empty["data"], serde_json::json!([]));
    assert_eq!(info_halaman(&empty), (1, 25, 0, 0));

    let created = ctx.banyak_pendaftaran(27).await;
    let newest_first: Vec<i64> = created.iter().rev().copied().collect();

    let list = halaman(&ctx, &panitia_magang, "").await;
    assert_eq!(ids(&list), newest_first[0..25]);
    assert_eq!(info_halaman(&list), (1, 25, 27, 2));

    // Only p means 25 rows per page.
    let list = halaman(&ctx, &panitia_magang, "?p=2").await;
    assert_eq!(ids(&list), newest_first[25..27]);
    assert_eq!(info_halaman(&list), (2, 25, 27, 2));
}

#[sqlx::test]
async fn only_r_means_the_first_page(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let created = ctx.banyak_pendaftaran(3).await;

    let list = halaman(&ctx, &panitia_magang, "?r=2").await;
    assert_eq!(ids(&list), [created[2], created[1]]);
    assert_eq!(info_halaman(&list), (1, 2, 3, 2));
}

#[sqlx::test]
async fn empty_list_has_no_pages_when_paged(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;

    let list = halaman(&ctx, &panitia_magang, "?p=1&r=10").await;
    assert_eq!(list["data"], serde_json::json!([]));
    assert_eq!(info_halaman(&list), (1, 10, 0, 0));
}

#[sqlx::test]
async fn pages_of_a_mahasiswa_only_count_their_own(db: PgPool) {
    let ctx = setup(db);
    ctx.banyak_pendaftaran(3).await;
    let (_, cookie) = ctx.mahasiswa("saya@student.ub.ac.id").await;
    let selesai = ctx.pendaftaran(&cookie, "Selesai").await;
    let lolos = ctx.pendaftaran(&cookie, LOLOS).await;

    let list = halaman(&ctx, &cookie, "?p=1&r=1").await;
    assert_eq!(info_halaman(&list), (1, 1, 2, 2));
    let list_2 = halaman(&ctx, &cookie, "?p=2&r=1").await;

    let mut mine = [ids(&list)[0], ids(&list_2)[0]];
    mine.sort_unstable();
    let mut expected = [selesai, lolos];
    expected.sort_unstable();
    assert_eq!(mine, expected);
}

#[sqlx::test]
async fn rejects_bad_p_and_r(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;

    for query in ["?p=0&r=10", "?p=1&r=0", "?p=-1&r=10", "?p=1&r=-5", "?r=0"] {
        let response = get(&ctx.app, Some(&panitia_magang), &format!("{URI}{query}")).await;
        assert_rejected(
            response,
            StatusCode::UNPROCESSABLE_ENTITY,
            "p dan r minimal 1.",
        )
        .await;
    }

    for query in [
        "?p=abc&r=10",
        "?p=1&r=abc",
        "?p=1.5&r=10",
        "?p=&r=10",
        "?p=99999999999&r=10",
        "?page=1&rows=10",
        "?periode=abc",
        "?status=Diterima",
        "?perusahaan=1.5",
        "?posisi=abc",
        "?status=",
        "?id_periode=1",
    ] {
        let response = get(&ctx.app, Some(&panitia_magang), &format!("{URI}{query}")).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{query}");
    }
}

#[sqlx::test]
async fn paging_keeps_the_fields_of_each_row(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, _) = ctx.pendaftaran_baru("a", LOLOS).await;
    let response = put_jadwal(&ctx.app, Some(&panitia_magang), jadwal_body(&[a])).await;
    assert_eq!(response.status(), StatusCode::OK);

    let paged = halaman(&ctx, &panitia_magang, "?p=1&r=1").await;
    let whole = halaman(&ctx, &panitia_magang, "").await;
    assert_eq!(paged["data"], whole["data"]);
    assert_eq!(paged["data"][0]["ada_jadwal_seleksi"], true);
}

#[sqlx::test]
async fn paging_needs_a_login_and_a_role(db: PgPool) {
    let ctx = setup(db);
    let user = insert_pengguna(&ctx.db, "tanpa.peran@gmail.com").await;
    let no_role = cookie_for(&ctx.state, &user);
    let uri = format!("{URI}?p=1&r=10");

    assert_eq!(
        get(&ctx.app, None, &uri).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get(&ctx.app, Some(&no_role), &uri).await.status(),
        StatusCode::FORBIDDEN
    );
}

/// Tiga posisi: A dan B satu periode, A dan C satu perusahaan.
struct PosisiFilter {
    a: i32,
    b: i32,
    c: i32,
    id_periode_ab: i32,
    id_periode_c: i32,
    id_perusahaan_ac: i32,
    id_perusahaan_b: i32,
}

impl Ctx {
    async fn posisi_filter(&self) -> PosisiFilter {
        let a = self.posisi_berjalan(50).await;
        let b = self.posisi_berjalan(50).await;
        let c = self.posisi_berjalan(50).await;

        let bagian = |id: i32| {
            let db = self.db.clone();
            async move {
                sqlx::query_as::<_, (i32, i32)>(
                    "SELECT id_periode, id_perusahaan FROM posisi_magang WHERE id_posisi = $1",
                )
                .bind(id)
                .fetch_one(&db)
                .await
                .unwrap()
            }
        };
        let (id_periode_ab, id_perusahaan_ac) = bagian(a).await;
        let (_, id_perusahaan_b) = bagian(b).await;
        let (id_periode_c, _) = bagian(c).await;

        sqlx::query("UPDATE posisi_magang SET id_periode = $2 WHERE id_posisi = $1")
            .bind(b)
            .bind(id_periode_ab)
            .execute(&self.db)
            .await
            .unwrap();
        sqlx::query("UPDATE posisi_magang SET id_perusahaan = $2 WHERE id_posisi = $1")
            .bind(c)
            .bind(id_perusahaan_ac)
            .execute(&self.db)
            .await
            .unwrap();

        PosisiFilter {
            a,
            b,
            c,
            id_periode_ab,
            id_periode_c,
            id_perusahaan_ac,
            id_perusahaan_b,
        }
    }

    async fn id_status(&self, status: &str) -> i32 {
        sqlx::query_scalar("SELECT id_status FROM status_pendaftaran WHERE status = $1")
            .bind(status)
            .fetch_one(&self.db)
            .await
            .unwrap()
    }

    /// Pendaftaran milik mahasiswa baru di posisi ini, dengan status ini.
    async fn pendaftaran_di(&self, nama: &str, id_posisi: i32, status: &str) -> i64 {
        let (_, cookie) = self.mahasiswa(&format!("{nama}@student.ub.ac.id")).await;
        let (id, _) = daftar_ok(self, &cookie, id_posisi).await;
        self.set_status(id, status).await;
        id
    }
}

fn sorted(mut ids: Vec<i64>) -> Vec<i64> {
    ids.sort_unstable();
    ids
}

#[sqlx::test]
async fn filters_each_narrow_the_list_and_its_totals(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let pos = ctx.posisi_filter().await;

    let a = ctx.pendaftaran_di("a", pos.a, LOLOS).await;
    let b = ctx.pendaftaran_di("b", pos.b, LOLOS).await;
    let c = ctx.pendaftaran_di("c", pos.c, "Diterima").await;
    let lolos = ctx.id_status(LOLOS).await;
    let diterima = ctx.id_status("Diterima").await;

    let cases = [
        (format!("?periode={}", pos.id_periode_ab), vec![a, b]),
        (format!("?periode={}", pos.id_periode_c), vec![c]),
        (format!("?perusahaan={}", pos.id_perusahaan_ac), vec![a, c]),
        (format!("?perusahaan={}", pos.id_perusahaan_b), vec![b]),
        (format!("?status={lolos}"), vec![a, b]),
        (format!("?status={diterima}"), vec![c]),
        (format!("?posisi={}", pos.a), vec![a]),
        (format!("?posisi={}", pos.c), vec![c]),
    ];
    for (query, expected) in cases {
        let list = halaman(&ctx, &panitia_magang, &query).await;
        assert_eq!(sorted(ids(&list)), sorted(expected.clone()), "{query}");
        assert_eq!(list["total"], expected.len(), "{query}");
        assert_eq!(list["total_pages"], 1, "{query}");
    }

    // Without a filter everything is there.
    let list = halaman(&ctx, &panitia_magang, "").await;
    assert_eq!(list["total"], 3);
}

#[sqlx::test]
async fn filters_sent_together_must_all_match(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let pos = ctx.posisi_filter().await;

    let a = ctx.pendaftaran_di("a", pos.a, LOLOS).await;
    ctx.pendaftaran_di("b", pos.b, LOLOS).await;
    let a_lain = ctx.pendaftaran_di("c", pos.a, "Diterima").await;
    ctx.pendaftaran_di("d", pos.c, LOLOS).await;
    let lolos = ctx.id_status(LOLOS).await;

    let query = format!(
        "?periode={}&perusahaan={}",
        pos.id_periode_ab, pos.id_perusahaan_ac
    );
    let list = halaman(&ctx, &panitia_magang, &query).await;
    assert_eq!(sorted(ids(&list)), sorted(vec![a, a_lain]));

    let query = format!(
        "?periode={}&status={lolos}&perusahaan={}",
        pos.id_periode_ab, pos.id_perusahaan_ac
    );
    let list = halaman(&ctx, &panitia_magang, &query).await;
    assert_eq!(ids(&list), [a]);
    assert_eq!(list["total"], 1);

    let query = format!("?posisi={}&status={lolos}", pos.a);
    let list = halaman(&ctx, &panitia_magang, &query).await;
    assert_eq!(ids(&list), [a]);

    let query = format!("?posisi={}&perusahaan={}", pos.a, pos.id_perusahaan_b);
    let list = halaman(&ctx, &panitia_magang, &query).await;
    assert_eq!(list["total"], 0);

    // A period and a company with nothing in common.
    let query = format!(
        "?periode={}&perusahaan={}",
        pos.id_periode_c, pos.id_perusahaan_b
    );
    let list = halaman(&ctx, &panitia_magang, &query).await;
    assert_eq!(list["data"], serde_json::json!([]));
    assert_eq!(list["total"], 0);
    assert_eq!(list["total_pages"], 0);
}

#[sqlx::test]
async fn pages_split_the_filtered_list(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let lolos = ctx.id_status(LOLOS).await;

    let created = ctx.banyak_pendaftaran(5).await;
    ctx.set_status(created[1], "Diterima").await;
    ctx.set_status(created[3], "Diterima").await;
    // Left with created[4], [2] and [0], newest first.

    let first = halaman(&ctx, &panitia_magang, &format!("?status={lolos}&p=1&r=2")).await;
    assert_eq!(ids(&first), [created[4], created[2]]);
    assert_eq!(info_halaman(&first), (1, 2, 3, 2));

    let second = halaman(&ctx, &panitia_magang, &format!("?status={lolos}&p=2&r=2")).await;
    assert_eq!(ids(&second), [created[0]]);
    assert_eq!(info_halaman(&second), (2, 2, 3, 2));
}

#[sqlx::test]
async fn filters_use_the_default_page_too(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let lolos = ctx.id_status(LOLOS).await;
    ctx.banyak_pendaftaran(27).await;

    let list = halaman(&ctx, &panitia_magang, &format!("?status={lolos}")).await;
    assert_eq!(ids(&list).len(), 25);
    assert_eq!(info_halaman(&list), (1, 25, 27, 2));
}

#[sqlx::test]
async fn filtering_on_an_unknown_id_finds_nothing(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    ctx.banyak_pendaftaran(2).await;

    for query in [
        "?periode=999999",
        "?status=999999",
        "?perusahaan=999999",
        "?posisi=999999",
        "?status=0",
        "?periode=-1",
    ] {
        let list = halaman(&ctx, &panitia_magang, query).await;
        assert_eq!(list["data"], serde_json::json!([]), "{query}");
        assert_eq!(list["total"], 0, "{query}");
    }
}

#[sqlx::test]
async fn a_mahasiswa_cannot_reach_others_through_a_filter(db: PgPool) {
    let ctx = setup(db);
    let id_posisi = ctx.posisi_berjalan(50).await;
    let (_, saya) = ctx.mahasiswa("saya@student.ub.ac.id").await;
    let (milik_saya, _) = daftar_ok(&ctx, &saya, id_posisi).await;
    ctx.set_status(milik_saya, LOLOS).await;
    ctx.pendaftaran_di("lain", id_posisi, LOLOS).await;
    let lolos = ctx.id_status(LOLOS).await;

    let (id_periode, id_perusahaan): (i32, i32) =
        sqlx::query_as("SELECT id_periode, id_perusahaan FROM posisi_magang WHERE id_posisi = $1")
            .bind(id_posisi)
            .fetch_one(&ctx.db)
            .await
            .unwrap();

    for query in [
        format!("?status={lolos}"),
        format!("?periode={id_periode}"),
        format!("?perusahaan={id_perusahaan}"),
        format!("?posisi={id_posisi}"),
    ] {
        let list = halaman(&ctx, &saya, &query).await;
        assert_eq!(ids(&list), [milik_saya], "{query}");
        assert_eq!(list["total"], 1, "{query}");
    }
}

#[sqlx::test]
async fn looking_up_one_pendaftaran_ignores_the_list_limits(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let created = ctx.banyak_pendaftaran(27).await;

    // The oldest one is not on the first page, but its detail is still there.
    let response = get(
        &ctx.app,
        Some(&panitia_magang),
        &format!("{URI}/{}", created[0]),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_body(response).await["id_pendaftaran"], created[0]);
}

impl Ctx {
    async fn pendaftaran_bernama(&self, nama: &str) -> i64 {
        let email = format!("cari.{:032x}", rand::random::<u128>());
        let (id, _) = self.pendaftaran_baru(&email, LOLOS).await;
        sqlx::query(
            "UPDATE mahasiswa SET nama = $2
             WHERE id_pengguna = (SELECT id_pengguna FROM pendaftaran WHERE id_pendaftaran = $1)",
        )
        .bind(id as i32)
        .bind(nama)
        .execute(&self.db)
        .await
        .unwrap();
        id
    }
}

#[sqlx::test]
async fn searching_finds_any_part_of_the_name_in_any_case(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let budi = ctx.pendaftaran_bernama("Budi Santoso").await;
    let abu = ctx.pendaftaran_bernama("Abu Budiman").await;
    let siti = ctx.pendaftaran_bernama("Siti Aminah").await;

    let cases = [
        ("?s=budi", vec![budi, abu]),
        ("?s=BUDI", vec![budi, abu]),
        ("?s=santoso", vec![budi]),
        ("?s=i", vec![budi, abu, siti]),
        ("?s=budi%20santoso", vec![budi]),
        ("?s=budi+santoso", vec![budi]),
        ("?s=%20%20aminah%20", vec![siti]),
        ("?s=santoso%20budi", vec![]),
        ("?s=tidak-ada", vec![]),
    ];
    for (query, expected) in cases {
        let list = halaman(&ctx, &panitia_magang, query).await;
        assert_eq!(sorted(ids(&list)), sorted(expected.clone()), "{query}");
        assert_eq!(list["total"], expected.len(), "{query}");
    }
}

#[sqlx::test]
async fn an_empty_search_is_ignored(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    ctx.pendaftaran_bernama("Budi").await;
    ctx.pendaftaran_bernama("Siti").await;

    for query in ["?s=", "?s=%20%20", "?s=+"] {
        let list = halaman(&ctx, &panitia_magang, query).await;
        assert_eq!(list["total"], 2, "{query}");
    }
}

#[sqlx::test]
async fn searching_treats_every_character_literally(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let persen = ctx.pendaftaran_bernama("Bu%di").await;
    let garis = ctx.pendaftaran_bernama("Bu_di").await;
    ctx.pendaftaran_bernama("Budi").await;

    let list = halaman(&ctx, &panitia_magang, "?s=%25").await;
    assert_eq!(ids(&list), [persen]);
    let list = halaman(&ctx, &panitia_magang, "?s=_").await;
    assert_eq!(ids(&list), [garis]);

    let list = halaman(&ctx, &panitia_magang, "?s=%27%20OR%201%3D1%20--").await;
    assert_eq!(list["total"], 0);
    assert_eq!(ctx.count("pendaftaran").await, 3);
}

#[sqlx::test]
async fn searching_works_with_filters_and_pages(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let lolos = ctx.id_status(LOLOS).await;

    let a = ctx.pendaftaran_bernama("Budi Satu").await;
    let b = ctx.pendaftaran_bernama("Budi Dua").await;
    let c = ctx.pendaftaran_bernama("Budi Tiga").await;
    ctx.pendaftaran_bernama("Siti Empat").await;
    ctx.set_status(c, "Diterima").await;

    let list = halaman(&ctx, &panitia_magang, &format!("?s=budi&status={lolos}")).await;
    assert_eq!(sorted(ids(&list)), sorted(vec![a, b]));
    assert_eq!(list["total"], 2);

    let first = halaman(&ctx, &panitia_magang, "?s=budi&p=1&r=2").await;
    assert_eq!(info_halaman(&first), (1, 2, 3, 2));
    let second = halaman(&ctx, &panitia_magang, "?s=budi&p=2&r=2").await;
    assert_eq!(info_halaman(&second), (2, 2, 3, 2));

    let mut semua = ids(&first);
    semua.extend(ids(&second));
    assert_eq!(sorted(semua), sorted(vec![a, b, c]));
}

#[sqlx::test]
async fn a_mahasiswa_only_searches_their_own(db: PgPool) {
    let ctx = setup(db);
    ctx.pendaftaran_bernama("Budi Lain").await;
    let (_, saya) = ctx.mahasiswa("saya@student.ub.ac.id").await;
    let milik_saya = ctx.pendaftaran(&saya, LOLOS).await;

    // The helper names every mahasiswa Budi.
    let list = halaman(&ctx, &saya, "?s=budi").await;
    assert_eq!(ids(&list), [milik_saya]);
    assert_eq!(list["total"], 1);
}

#[sqlx::test]
async fn a_pendaftaran_without_a_name_is_not_found_by_searching(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let id = ctx.pendaftaran_bernama("Budi").await;
    sqlx::query("UPDATE mahasiswa SET nama = NULL")
        .execute(&ctx.db)
        .await
        .unwrap();

    let list = halaman(&ctx, &panitia_magang, "?s=budi").await;
    assert_eq!(list["total"], 0);

    let list = halaman(&ctx, &panitia_magang, "").await;
    assert_eq!(ids(&list), [id]);
}

const PLOT_URI: &str = "/api/v1/pendaftaran/plot-dpm";
const BUKAN_AKTIF: &str = "Semua pendaftaran harus berstatus Aktif.";

async fn put_plot_raw(
    app: &Router,
    cookie: Option<&str>,
    content_type: Option<&str>,
    body: String,
) -> Response<Body> {
    let mut request = Request::builder().method("PUT").uri(PLOT_URI);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    if let Some(content_type) = content_type {
        request = request.header(header::CONTENT_TYPE, content_type);
    }
    app.clone()
        .oneshot(request.body(Body::from(body)).unwrap())
        .await
        .unwrap()
}

async fn put_plot(app: &Router, cookie: Option<&str>, body: Value) -> Response<Body> {
    put_plot_raw(app, cookie, Some("application/json"), body.to_string()).await
}

fn plot_body(ids: &[i64]) -> Value {
    serde_json::json!({
        "id_pendaftaran": ids,
        "nama_dpm": "Dr. Budi",
        "kontak_dpm": "budi@ub.ac.id",
    })
}

fn plot_awal() -> Option<(String, String)> {
    Some(("Dr. Budi".to_string(), "budi@ub.ac.id".to_string()))
}

impl Ctx {
    async fn plotting_of(&self, id: i64) -> Option<(String, String)> {
        sqlx::query_as("SELECT nama_dpm, kontak_dpm FROM plotting_dpm WHERE id_pendaftaran = $1")
            .bind(id as i32)
            .fetch_optional(&self.db)
            .await
            .unwrap()
    }
}

#[sqlx::test]
async fn saves_the_same_dpm_on_every_pendaftaran_listed(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, cookie_a) = ctx.pendaftaran_baru("a", "Aktif").await;
    let (b, _) = ctx.pendaftaran_baru("b", "Aktif").await;
    let (c, _) = ctx.pendaftaran_baru("c", "Aktif").await;
    let (bukan, _) = ctx.pendaftaran_baru("d", "Aktif").await;
    let before = ctx.snapshot(a).await;
    let kuota_sebelum: Vec<i32> =
        sqlx::query_scalar("SELECT kuota FROM posisi_magang ORDER BY id_posisi")
            .fetch_all(&ctx.db)
            .await
            .unwrap();

    let response = put_plot(&ctx.app, Some(&panitia_magang), plot_body(&[c, a, b])).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await,
        serde_json::json!({
            "id_pendaftaran": [a, b, c],
            "nama_dpm": "Dr. Budi",
            "kontak_dpm": "budi@ub.ac.id",
        })
    );

    for id in [a, b, c] {
        assert_eq!(ctx.plotting_of(id).await, plot_awal());
    }
    assert_eq!(ctx.plotting_of(bukan).await, None);

    // The owner and Panitia Magang both see it, in the detail and as a flag in the list.
    for cookie in [&cookie_a, &panitia_magang] {
        let response = get(&ctx.app, Some(cookie), &format!("{URI}/{a}")).await;
        assert_eq!(
            json_body(response).await["plotting_dpm"],
            serde_json::json!({ "nama_dpm": "Dr. Budi", "kontak_dpm": "budi@ub.ac.id" })
        );
        assert_eq!(
            flags_in_list(&ctx, cookie, a).await,
            (Value::Bool(false), Value::Bool(true))
        );
    }

    // The status and its log stay, while updated_at moves on.
    let after = ctx.snapshot(a).await;
    assert_eq!(after.0, before.0);
    assert_eq!(after.2, before.2);
    assert_ne!(after.1, before.1);
    let kuota_sesudah: Vec<i32> =
        sqlx::query_scalar("SELECT kuota FROM posisi_magang ORDER BY id_posisi")
            .fetch_all(&ctx.db)
            .await
            .unwrap();
    assert_eq!(kuota_sesudah, kuota_sebelum);
}

#[sqlx::test]
async fn plotting_again_replaces_the_dpm(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (lama, _) = ctx.pendaftaran_baru("a", "Aktif").await;
    let (baru, _) = ctx.pendaftaran_baru("b", "Aktif").await;

    let response = put_plot(&ctx.app, Some(&panitia_magang), plot_body(&[lama])).await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = serde_json::json!({
        "id_pendaftaran": [lama, baru],
        "nama_dpm": "Dr. Siti",
        "kontak_dpm": "081234567890",
    });
    let response = put_plot(&ctx.app, Some(&panitia_magang), body).await;
    assert_eq!(response.status(), StatusCode::OK);

    let expected = Some(("Dr. Siti".to_string(), "081234567890".to_string()));
    assert_eq!(ctx.plotting_of(lama).await, expected);
    assert_eq!(ctx.plotting_of(baru).await, expected);
    assert_eq!(ctx.count("plotting_dpm").await, 2);
}

#[sqlx::test]
async fn plotting_an_id_listed_twice_counts_once(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, _) = ctx.pendaftaran_baru("a", "Aktif").await;

    let response = put_plot(&ctx.app, Some(&panitia_magang), plot_body(&[a, a, a])).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await["id_pendaftaran"],
        serde_json::json!([a])
    );
    assert_eq!(ctx.count("plotting_dpm").await, 1);
}

#[sqlx::test]
async fn one_pendaftaran_not_aktif_stops_the_whole_plotting(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;

    for status in STATUSES.into_iter().filter(|s| *s != "Aktif") {
        let nama = status.replace(['/', ' '], ".");
        let (punya, _) = ctx.pendaftaran_baru(&format!("{nama}.a"), "Aktif").await;
        let (aktif, _) = ctx.pendaftaran_baru(&format!("{nama}.b"), "Aktif").await;
        let (lain, _) = ctx.pendaftaran_baru(&format!("{nama}.c"), status).await;

        let response = put_plot(&ctx.app, Some(&panitia_magang), plot_body(&[punya])).await;
        assert_eq!(response.status(), StatusCode::OK);
        let before = ctx.snapshot(punya).await;

        let body = serde_json::json!({
            "id_pendaftaran": [punya, aktif, lain],
            "nama_dpm": "Dr. Siti",
            "kontak_dpm": "siti@ub.ac.id",
        });
        let response = put_plot(&ctx.app, Some(&panitia_magang), body).await;
        assert_rejected(response, StatusCode::CONFLICT, BUKAN_AKTIF).await;

        assert_eq!(ctx.plotting_of(punya).await, plot_awal(), "{status}");
        assert_eq!(ctx.plotting_of(aktif).await, None, "{status}");
        assert_eq!(ctx.plotting_of(lain).await, None, "{status}");
        assert_eq!(ctx.snapshot(punya).await, before, "{status}");
    }
}

#[sqlx::test]
async fn one_missing_pendaftaran_stops_the_whole_plotting(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, _) = ctx.pendaftaran_baru("a", "Aktif").await;

    let response = put_plot(&ctx.app, Some(&panitia_magang), plot_body(&[a, 999_999])).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(ctx.plotting_of(a).await, None);
}

#[sqlx::test]
async fn rejects_bad_plotting_without_saving(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, _) = ctx.pendaftaran_baru("a", "Aktif").await;
    let before = ctx.snapshot(a).await;
    let nama_salah = "Nama DPM harus berisi 1-255 karakter.";
    let kontak_salah = "Kontak DPM harus berisi 1-255 karakter.";

    let cases = [
        (
            "id_pendaftaran",
            serde_json::json!([]),
            "Pilih minimal satu pendaftaran.",
        ),
        ("nama_dpm", serde_json::json!(""), nama_salah),
        ("nama_dpm", serde_json::json!("   "), nama_salah),
        ("nama_dpm", serde_json::json!("x".repeat(256)), nama_salah),
        ("kontak_dpm", serde_json::json!(""), kontak_salah),
        ("kontak_dpm", serde_json::json!("   "), kontak_salah),
        (
            "kontak_dpm",
            serde_json::json!("x".repeat(256)),
            kontak_salah,
        ),
    ];
    for (field, value, message) in cases {
        let mut body = plot_body(&[a]);
        body[field] = value;
        let response = put_plot(&ctx.app, Some(&panitia_magang), body).await;
        assert_rejected(response, StatusCode::UNPROCESSABLE_ENTITY, message).await;
    }

    // The framework's own rejections, which have no message.
    for field in ["id_pendaftaran", "nama_dpm", "kontak_dpm"] {
        let mut body = plot_body(&[a]);
        body.as_object_mut().unwrap().remove(field);
        let response = put_plot(&ctx.app, Some(&panitia_magang), body).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{field}"
        );
    }
    let mut extra = plot_body(&[a]);
    extra["status"] = serde_json::json!("Selesai");
    let mut satu_id = plot_body(&[a]);
    satu_id["id_pendaftaran"] = serde_json::json!(a);
    for body in [extra, satu_id] {
        let response = put_plot(&ctx.app, Some(&panitia_magang), body).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    let response = put_plot_raw(
        &ctx.app,
        Some(&panitia_magang),
        Some("application/json"),
        "{".to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = plot_body(&[a]).to_string();
    let response = put_plot_raw(&ctx.app, Some(&panitia_magang), None, body).await;
    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    assert_eq!(ctx.plotting_of(a).await, None);
    assert_eq!(ctx.snapshot(a).await, before);
}

#[sqlx::test]
async fn plotting_trims_and_accepts_the_longest_values(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, _) = ctx.pendaftaran_baru("a", "Aktif").await;
    let panjang = "é".repeat(255);

    let body = serde_json::json!({
        "id_pendaftaran": [a],
        "nama_dpm": format!("  {panjang}  "),
        "kontak_dpm": format!(" {panjang} "),
    });
    let response = put_plot(&ctx.app, Some(&panitia_magang), body).await;
    assert_eq!(response.status(), StatusCode::OK);
    let saved = json_body(response).await;
    assert_eq!(saved["nama_dpm"], panjang.as_str());
    assert_eq!(
        ctx.plotting_of(a).await,
        Some((panjang.clone(), panjang.clone()))
    );
}

#[sqlx::test]
async fn only_panitia_magang_can_save_a_plotting(db: PgPool) {
    let ctx = setup(db);
    let (a, owner) = ctx.pendaftaran_baru("a", "Aktif").await;
    let (_, other) = ctx.mahasiswa("b@student.ub.ac.id").await;
    let user = insert_pengguna(&ctx.db, "tanpa.peran@gmail.com").await;
    let no_role = cookie_for(&ctx.state, &user);

    let response = put_plot(&ctx.app, None, plot_body(&[a])).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let bad = Some("access_token=bukan-jwt");
    let response = put_plot(&ctx.app, bad, plot_body(&[a])).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    for cookie in [&owner, &other, &no_role] {
        let response = put_plot(&ctx.app, Some(cookie), plot_body(&[a])).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    assert_eq!(ctx.plotting_of(a).await, None);
}

#[sqlx::test]
async fn overlapping_plottings_end_with_one_dpm_for_all(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, _) = ctx.pendaftaran_baru("a", "Aktif").await;
    let (b, _) = ctx.pendaftaran_baru("b", "Aktif").await;

    let isi = |ids: [i64; 2], nama: &str| {
        serde_json::json!({
            "id_pendaftaran": ids,
            "nama_dpm": nama,
            "kontak_dpm": "kontak",
        })
    };
    let (first, second) = tokio::join!(
        put_plot(&ctx.app, Some(&panitia_magang), isi([a, b], "Satu")),
        put_plot(&ctx.app, Some(&panitia_magang), isi([b, a], "Dua")),
    );

    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(ctx.plotting_of(a).await, ctx.plotting_of(b).await);
    assert_eq!(ctx.count("plotting_dpm").await, 2);
}

#[sqlx::test]
async fn plotting_sees_a_status_changed_while_it_waited(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;

    let (a, _) = ctx.pendaftaran_baru("a", "Aktif").await;
    let request = put_plot(&ctx.app, Some(&panitia_magang), plot_body(&[a]));
    let response = ctx.while_status_changes_to(a, "Selesai", request).await;
    assert_rejected(response, StatusCode::CONFLICT, BUKAN_AKTIF).await;
    assert_eq!(ctx.plotting_of(a).await, None);

    let (b, _) = ctx.pendaftaran_baru("b", "Diterima").await;
    let request = put_plot(&ctx.app, Some(&panitia_magang), plot_body(&[b]));
    let response = ctx.while_status_changes_to(b, "Aktif", request).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(ctx.plotting_of(b).await, plot_awal());
}

#[sqlx::test]
async fn the_route_does_not_get_in_the_way_of_the_detail(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;

    let response = get(&ctx.app, Some(&panitia_magang), PLOT_URI).await;
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

const EKSPOR_URI: &str = "/api/v1/pendaftaran/ekspor";

async fn post_ekspor(app: &Router, cookie: Option<&str>, body: Value) -> Response<Body> {
    let mut request = Request::builder()
        .method("POST")
        .uri(EKSPOR_URI)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    app.clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

/// Bagian berkas .xlsx yang diperiksa oleh test.
struct Xlsx {
    sheet: String,
    strings: String,
    /// Tujuan tautan di lembar pertama, sesuai urutan ditulis.
    links: Vec<String>,
}

fn baca_xlsx(isi: &[u8]) -> Xlsx {
    use std::io::Read;

    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(isi)).unwrap();
    let mut baca = |nama: &str| {
        let mut teks = String::new();
        if let Ok(mut file) = zip.by_name(nama) {
            file.read_to_string(&mut teks).unwrap();
        }
        teks
    };
    let sheet = baca("xl/worksheets/sheet1.xml");
    let strings = baca("xl/sharedStrings.xml");
    let rels = baca("xl/worksheets/_rels/sheet1.xml.rels");
    let links = rels
        .split("Target=\"")
        .skip(1)
        .filter_map(|bagian| bagian.split('"').next())
        .filter(|target| target.starts_with("http"))
        .map(str::to_string)
        .collect();
    Xlsx {
        sheet,
        strings,
        links,
    }
}

/// Token dari tautan publik yang dibuat oleh ekspor.
fn token_of(link: &str) -> &str {
    link.strip_prefix("http://frontend.test/berkas/")
        .unwrap_or_else(|| panic!("{link}"))
}

async fn get_publik(app: &Router, token: &str) -> Response<Body> {
    get(app, None, &format!("/api/v1/berkas/publik/{token}")).await
}

impl Ctx {
    async fn nim_of(&self, id: i64) -> String {
        sqlx::query_scalar(
            "SELECT m.nim::text
             FROM pendaftaran p
             JOIN mahasiswa m ON m.id_pengguna = p.id_pengguna
             WHERE p.id_pendaftaran = $1",
        )
        .bind(id as i32)
        .fetch_one(&self.db)
        .await
        .unwrap()
    }
}

#[sqlx::test]
async fn panitia_magang_exports_the_picked_pendaftaran_as_xlsx(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (a, _) = ctx
        .pendaftaran_baru("a", "Lolos Seleksi Administrasi")
        .await;
    let (b, _) = ctx.pendaftaran_baru("b", "Aktif").await;
    let (lain, _) = ctx.pendaftaran_baru("c", "Menunggu Verifikasi").await;

    sqlx::query(
        "INSERT INTO jadwal_seleksi_substansi (id_pendaftaran, tanggal, jam, lokasi)
         VALUES ($1, '2099-12-01', '13:30', 'Gedung F, Ruang F2.3')",
    )
    .bind(a as i32)
    .execute(&ctx.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO plotting_dpm (id_pendaftaran, nama_dpm, kontak_dpm)
         VALUES ($1, 'Dr. Budi', 'budi@ub.ac.id')",
    )
    .bind(b as i32)
    .execute(&ctx.db)
    .await
    .unwrap();
    // Text from a mahasiswa that looks like a formula.
    sqlx::query(
        "UPDATE mahasiswa SET nama = '=HYPERLINK(\"http://jahat.test\")'
         WHERE id_pengguna = (SELECT id_pengguna FROM pendaftaran WHERE id_pendaftaran = $1)",
    )
    .bind(a as i32)
    .execute(&ctx.db)
    .await
    .unwrap();
    let before = ctx.snapshot(a).await;

    let body = serde_json::json!({ "format": "xlsx", "id_pendaftaran": [a, b, a] });
    let response = post_ekspor(&ctx.app, Some(&panitia_magang), body).await;

    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert_eq!(
        headers[header::CONTENT_TYPE],
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    );
    let disposition = headers[header::CONTENT_DISPOSITION].to_str().unwrap();
    assert!(
        disposition.starts_with("attachment; filename=\"pendaftar-")
            && disposition.ends_with(".xlsx\""),
        "{disposition}"
    );
    assert_eq!(headers[header::CACHE_CONTROL], "private, no-store");

    let isi = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let Xlsx {
        sheet,
        strings,
        links,
    } = baca_xlsx(&isi);

    // A header and one row for each pendaftaran, with the duplicate id counted once.
    assert_eq!(sheet.matches("<row ").count(), 3);
    assert!(!sheet.contains("<f>"), "no cell may hold a formula");
    assert!(strings.contains("=HYPERLINK("));

    for teks in [
        "Nama",
        "Nomor Surat Keterangan",
        "Laki-laki",
        "Teknik Informatika",
        "081234567890",
        "Backend",
        "PT Satu",
        "Aktif",
        "Lolos Seleksi Administrasi",
        "Gedung F, Ruang F2.3",
        "Dr. Budi",
        "budi@ub.ac.id",
        "a@student.ub.ac.id",
        "URL Surat Pernyataan DPA",
        "URL CV",
    ] {
        assert!(strings.contains(teks), "{teks}");
    }
    assert!(!strings.contains("Menunggu Verifikasi"));

    // Newest first, as in the list: b was registered after a.
    let nim_a = ctx.nim_of(a).await;
    let nim_b = ctx.nim_of(b).await;
    let nim_lain = ctx.nim_of(lain).await;
    let posisi = |nim: &str| strings.find(&format!(">{nim}<"));
    assert!(posisi(&nim_b).unwrap() < posisi(&nim_a).unwrap());
    assert_eq!(posisi(&nim_lain), None);

    // Two files for each of the two rows, each opening without a login.
    assert_eq!(links.len(), 4);
    for (link, isi) in links
        .iter()
        .zip([pdf("surat"), pdf("cv"), pdf("surat"), pdf("cv")])
    {
        let response = get_publik(&ctx.app, token_of(link)).await;
        assert_eq!(response.status(), StatusCode::OK, "{link}");
        assert_eq!(
            to_bytes(response.into_body(), usize::MAX).await.unwrap(),
            isi
        );
    }

    // Exporting changes nothing.
    assert_eq!(ctx.snapshot(a).await, before);
}

#[sqlx::test]
async fn a_public_link_opens_its_file_without_logging_in(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("m@student.ub.ac.id").await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let created = json_body(ctx.daftar(&cookie, id_posisi).await).await;
    let id_cv = created["berkas"][1]["id_berkas"].as_i64().unwrap() as i32;
    let nim = ctx
        .nim_of(created["id_pendaftaran"].as_i64().unwrap())
        .await;

    let besok = time::OffsetDateTime::now_utc().unix_timestamp() + 86_400;
    let token = ctx.state.tautan_berkas.buat(id_cv, besok);
    let response = get_publik(&ctx.app, &token).await;

    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert_eq!(headers[header::CONTENT_TYPE], "application/pdf");
    assert_eq!(
        headers[header::CONTENT_DISPOSITION],
        format!("inline; filename=\"cv-{nim}.pdf\"").as_str()
    );
    assert_eq!(headers[header::CACHE_CONTROL], "private, no-store");
    assert_eq!(headers["x-robots-tag"], "noindex");
    assert_eq!(
        to_bytes(response.into_body(), usize::MAX).await.unwrap(),
        pdf("cv")
    );
}

#[sqlx::test]
async fn a_public_link_that_is_changed_expired_or_foreign_finds_nothing(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("m@student.ub.ac.id").await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let created = json_body(ctx.daftar(&cookie, id_posisi).await).await;
    let id_surat = created["berkas"][0]["id_berkas"].as_i64().unwrap() as i32;
    let id_cv = created["berkas"][1]["id_berkas"].as_i64().unwrap() as i32;

    let sekarang = time::OffsetDateTime::now_utc().unix_timestamp();
    let token = ctx.state.tautan_berkas.buat(id_surat, sekarang + 86_400);
    let (isi, tag) = token.rsplit_once('.').unwrap();
    let kedaluwarsa = isi.split_once('.').unwrap().1;

    for palsu in [
        // Another file, or a later expiry, under the same tag.
        format!("{id_cv}.{kedaluwarsa}.{tag}"),
        format!("{id_surat}.{}.{tag}", sekarang + 999_999),
        ctx.state.tautan_berkas.buat(id_surat, sekarang - 1),
        super::TautanBerkas::new("another-secret-with-at-least-32-bytes")
            .buat(id_surat, sekarang + 86_400),
        ctx.state.tautan_berkas.buat(9999, sekarang + 86_400),
        id_surat.to_string(),
        "bukan-token".to_string(),
    ] {
        let response = get_publik(&ctx.app, &palsu).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{palsu}");
    }

    // The ordinary route still asks for a login.
    let response = get(&ctx.app, None, &format!("/api/v1/berkas/{id_surat}")).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn rejects_bad_export_requests(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (id, _) = ctx.pendaftaran_baru("a", "Aktif").await;

    let response = post_ekspor(
        &ctx.app,
        Some(&panitia_magang),
        serde_json::json!({ "format": "xlsx", "id_pendaftaran": [] }),
    )
    .await;
    assert_rejected(
        response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "Pilih minimal satu pendaftaran.",
    )
    .await;

    let terlalu_banyak: Vec<i32> = (1..=10_001).collect();
    let response = post_ekspor(
        &ctx.app,
        Some(&panitia_magang),
        serde_json::json!({ "format": "xlsx", "id_pendaftaran": terlalu_banyak }),
    )
    .await;
    assert_rejected(
        response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "Maksimal 10000 pendaftaran dalam satu ekspor.",
    )
    .await;

    // One unknown id fails the whole export.
    let response = post_ekspor(
        &ctx.app,
        Some(&panitia_magang),
        serde_json::json!({ "format": "xlsx", "id_pendaftaran": [id, 9999] }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    for body in [
        serde_json::json!({ "format": "csv", "id_pendaftaran": [id] }),
        serde_json::json!({ "id_pendaftaran": [id] }),
        serde_json::json!({ "format": "xlsx", "id_pendaftaran": [id], "lain": 1 }),
        serde_json::json!({ "format": "xlsx", "id_pendaftaran": ["1"] }),
    ] {
        let response = post_ekspor(&ctx.app, Some(&panitia_magang), body.clone()).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}"
        );
    }
}

#[sqlx::test]
async fn only_panitia_magang_can_export(db: PgPool) {
    let ctx = setup(db);
    let (id, cookie) = ctx.pendaftaran_baru("a", "Aktif").await;
    let body = serde_json::json!({ "format": "xlsx", "id_pendaftaran": [id] });

    let response = post_ekspor(&ctx.app, None, body.clone()).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // Not even a mahasiswa exporting their own pendaftaran.
    let response = post_ekspor(&ctx.app, Some(&cookie), body).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn rejects_a_position_of_a_hidden_company(db: PgPool) {
    let ctx = setup(db);
    let (_, cookie) = ctx.mahasiswa("m@student.ub.ac.id").await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    sqlx::query(
        "UPDATE perusahaan_mitra SET deleted_at = now()
         WHERE id_perusahaan = (SELECT id_perusahaan FROM posisi_magang WHERE id_posisi = $1)",
    )
    .bind(id_posisi)
    .execute(&ctx.db)
    .await
    .unwrap();

    let response = ctx.daftar(&cookie, id_posisi).await;
    assert_rejected(
        response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "Posisi tidak ditemukan.",
    )
    .await;

    ctx.assert_nothing_saved().await;
}

#[sqlx::test]
async fn a_hidden_position_takes_no_new_pendaftaran_but_keeps_the_old_ones(db: PgPool) {
    let ctx = setup(db);
    let panitia_magang = ctx.panitia_magang().await;
    let (_, lama) = ctx.mahasiswa("lama@student.ub.ac.id").await;
    let (_, baru) = ctx.mahasiswa("baru@student.ub.ac.id").await;
    let id_posisi = ctx.posisi_berjalan(5).await;
    let (id, _) = daftar_ok(&ctx, &lama, id_posisi).await;

    sqlx::query("UPDATE posisi_magang SET deleted_at = now() WHERE id_posisi = $1")
        .bind(id_posisi)
        .execute(&ctx.db)
        .await
        .unwrap();

    let response = ctx.daftar(&baru, id_posisi).await;
    assert_rejected(
        response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "Posisi tidak ditemukan.",
    )
    .await;

    // The pendaftaran made before can still be seen and processed.
    let list = json_body(get(&ctx.app, Some(&panitia_magang), URI).await).await;
    assert_eq!(ids(&list), [id]);
    let response = ubah_status(&ctx.app, &panitia_magang, id, LOLOS).await;
    assert_eq!(response.status(), StatusCode::OK);
}
