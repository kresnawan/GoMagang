use std::path::PathBuf;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, Response, StatusCode, header},
};
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;

use crate::{app, config::Config, features::auth::User, state::AppState};

const KUNCI: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
const KUNCI_LAIN: &str = "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=";
const BOUNDARY: &str = "batas-pengujian";

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let name = format!("gomagang-surat-{:032x}", rand::random::<u128>());
        Self(std::env::temp_dir().join(name))
    }

    fn files(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.0) else {
            return Vec::new();
        };
        entries
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect()
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
}

fn config(uploads: &TempDir, kunci: Option<&str>) -> Config {
    Config {
        database_url: String::new(),
        cors_allowed_origins: vec![],
        google_client_id: "client".to_string(),
        google_client_secret: "secret".to_string(),
        google_redirect_uri: "http://backend.test/auth/google".to_string(),
        frontend_url: "http://frontend.test".to_string(),
        jwt_secret: "test-secret-with-at-least-32-bytes!".to_string(),
        admin_secret: None,
        cookie_secure: false,
        upload_dir: uploads.0.clone(),
        surat_signing_key: kunci.map(str::to_string),
        smtp: None,
    }
}

fn setup_with_key(db: PgPool, kunci: Option<&str>) -> Ctx {
    let uploads = TempDir::new();
    let config = config(&uploads, kunci);
    let state = AppState::new(&config, db.clone());
    Ctx {
        app: app::build(&config, state.clone()),
        state,
        db,
        uploads,
    }
}

fn setup(db: PgPool) -> Ctx {
    setup_with_key(db, Some(KUNCI))
}

impl Ctx {
    async fn pengguna(&self, email: &str) -> User {
        let id_pengguna = sqlx::query_scalar(
            "INSERT INTO pengguna (email, google_sub) VALUES ($1, $1) RETURNING id_pengguna",
        )
        .bind(email)
        .fetch_one(&self.db)
        .await
        .unwrap();
        User {
            id_pengguna,
            email: email.to_string(),
        }
    }

    fn cookie(&self, user: &User) -> String {
        let cookie = self.state.sessions.access_cookie(user).unwrap();
        format!("{}={}", cookie.name(), cookie.value())
    }

    async fn mahasiswa(&self, email: &str, nim: &str) -> (User, String) {
        let user = self.pengguna(email).await;
        sqlx::query(
            "INSERT INTO mahasiswa
                 (id_pengguna, nim, nama, program_studi, ipk, no_hp, jenis_kelamin, semester)
             VALUES ($1, $2, 'Budi Santoso', 'Sistem Informasi', 3.5, '081234567890', 'L', 6)",
        )
        .bind(user.id_pengguna)
        .bind(nim)
        .execute(&self.db)
        .await
        .unwrap();
        let cookie = self.cookie(&user);
        (user, cookie)
    }

    async fn panitia_magang(&self) -> String {
        let user = self.pengguna("panitia@gmail.com").await;
        sqlx::query("INSERT INTO panitia_magang (id_pengguna) VALUES ($1)")
            .bind(user.id_pengguna)
            .execute(&self.db)
            .await
            .unwrap();
        self.cookie(&user)
    }

    /// A pendaftaran of this mahasiswa with the given status.
    async fn pendaftaran(&self, user: &User, status: &str) -> i32 {
        sqlx::query_scalar(
            "WITH perusahaan AS (
                 INSERT INTO perusahaan_mitra (nama_perusahaan) VALUES ('PT Nusantara Digital')
                 RETURNING id_perusahaan
             ), periode AS (
                 INSERT INTO periode_pendaftaran (waktu_mulai, waktu_tutup)
                 VALUES (now() - interval '2 day', now() - interval '1 day')
                 RETURNING id_periode
             ), posisi AS (
                 INSERT INTO posisi_magang
                     (id_perusahaan, posisi, deskripsi_persyaratan, kuota, id_periode)
                 SELECT id_perusahaan, 'Intern Business Analyst', '-', 5, id_periode
                 FROM perusahaan, periode
                 RETURNING id_posisi
             )
             INSERT INTO pendaftaran (id_posisi, id_status, id_pengguna)
             SELECT id_posisi, (SELECT id_status FROM status_pendaftaran WHERE status = $2), $1
             FROM posisi
             RETURNING id_pendaftaran",
        )
        .bind(user.id_pengguna)
        .bind(status)
        .fetch_one(&self.db)
        .await
        .unwrap()
    }

    async fn set_status(&self, id_pendaftaran: i32, status: &str) {
        sqlx::query(
            "UPDATE pendaftaran
             SET id_status = (SELECT id_status FROM status_pendaftaran WHERE status = $2)
             WHERE id_pendaftaran = $1",
        )
        .bind(id_pendaftaran)
        .bind(status)
        .execute(&self.db)
        .await
        .unwrap();
    }

    async fn unduh(&self, cookie: Option<&str>, id_pendaftaran: i32) -> Response<Body> {
        let mut request = Request::builder().uri(format!(
            "/api/v1/pendaftaran/{id_pendaftaran}/surat-keterangan"
        ));
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        self.app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn unduh_ok(&self, cookie: &str, id_pendaftaran: i32) -> Vec<u8> {
        let response = self.unduh(Some(cookie), id_pendaftaran).await;
        assert_eq!(response.status(), StatusCode::OK);
        bytes(response).await
    }

    async fn verifikasi(&self, kode: &str) -> Response<Body> {
        let uri = format!("/api/v1/surat-keterangan/verifikasi?kode={kode}");
        self.app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn verifikasi_berkas(&self, isi: &[u8]) -> Response<Body> {
        let mut body = format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"berkas\"; \
             filename=\"surat.pdf\"\r\nContent-Type: application/pdf\r\n\r\n"
        )
        .into_bytes();
        body.extend_from_slice(isi);
        body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/surat-keterangan/verifikasi")
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={BOUNDARY}"),
            )
            .body(Body::from(body))
            .unwrap();
        self.app.clone().oneshot(request).await.unwrap()
    }

    async fn kode(&self, id_pendaftaran: i32) -> String {
        let (id_surat, tanda_tangan): (i32, String) = sqlx::query_as(
            "SELECT id_surat, tanda_tangan FROM surat_keterangan WHERE id_pendaftaran = $1",
        )
        .bind(id_pendaftaran)
        .fetch_one(&self.db)
        .await
        .unwrap();
        format!("{id_surat}.{tanda_tangan}")
    }

    async fn count_surat(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM surat_keterangan")
            .fetch_one(&self.db)
            .await
            .unwrap()
    }
}

async fn bytes(response: Response<Body>) -> Vec<u8> {
    to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec()
}

async fn json_body(response: Response<Body>) -> Value {
    serde_json::from_slice(&bytes(response).await).unwrap()
}

#[sqlx::test]
async fn the_first_download_of_an_aktif_pendaftaran_issues_a_signed_pdf(db: PgPool) {
    let ctx = setup(db);
    let (user, cookie) = ctx
        .mahasiswa("budi@student.ub.ac.id", "215150200111001")
        .await;
    let id = ctx.pendaftaran(&user, "Aktif").await;

    let response = ctx.unduh(Some(&cookie), id).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/pdf");
    assert_eq!(
        response.headers()[header::CONTENT_DISPOSITION],
        "attachment; filename=\"surat-keterangan-magang-215150200111001.pdf\""
    );
    assert_eq!(
        response.headers()[header::CACHE_CONTROL],
        "private, no-store"
    );
    let pdf = bytes(response).await;
    assert!(pdf.starts_with(b"%PDF-"));

    assert_eq!(ctx.count_surat().await, 1);
    assert_eq!(ctx.uploads.files().len(), 1);

    let (isi, nomor_surat, hash): (String, String, String) = sqlx::query_as(
        "SELECT isi, nomor_surat, hash_berkas FROM surat_keterangan WHERE id_pendaftaran = $1",
    )
    .bind(id)
    .fetch_one(&ctx.db)
    .await
    .unwrap();
    let isi: Value = serde_json::from_str(&isi).unwrap();
    assert_eq!(isi["nama"], "Budi Santoso");
    assert_eq!(isi["nim"], "215150200111001");
    assert_eq!(isi["program_studi"], "S1 Sistem Informasi");
    assert_eq!(isi["perusahaan"], "PT Nusantara Digital");
    assert_eq!(isi["posisi"], "Intern Business Analyst");
    assert_eq!(isi["nomor_surat"], nomor_surat);
    assert_eq!(hash.len(), 64);
}

#[sqlx::test]
async fn later_downloads_send_the_same_file_even_after_the_data_changes(db: PgPool) {
    let ctx = setup(db);
    let (user, cookie) = ctx
        .mahasiswa("budi@student.ub.ac.id", "215150200111001")
        .await;
    let id = ctx.pendaftaran(&user, "Aktif").await;

    let pertama = ctx.unduh_ok(&cookie, id).await;
    sqlx::query("UPDATE mahasiswa SET nama = 'Nama Baru' WHERE id_pengguna = $1")
        .bind(user.id_pengguna)
        .execute(&ctx.db)
        .await
        .unwrap();
    ctx.set_status(id, "Selesai").await;

    assert_eq!(ctx.unduh_ok(&cookie, id).await, pertama);
    assert_eq!(ctx.count_surat().await, 1);
    assert_eq!(ctx.uploads.files().len(), 1);
}

#[sqlx::test]
async fn concurrent_first_downloads_issue_one_surat(db: PgPool) {
    let ctx = setup(db);
    let (user, cookie) = ctx
        .mahasiswa("budi@student.ub.ac.id", "215150200111001")
        .await;
    let id = ctx.pendaftaran(&user, "Aktif").await;

    let (a, b) = tokio::join!(ctx.unduh_ok(&cookie, id), ctx.unduh_ok(&cookie, id));

    assert_eq!(a, b);
    assert_eq!(ctx.count_surat().await, 1);
    assert_eq!(ctx.uploads.files().len(), 1);
}

#[sqlx::test]
async fn only_an_aktif_pendaftaran_gets_a_new_surat(db: PgPool) {
    let ctx = setup(db);
    let (user, cookie) = ctx
        .mahasiswa("budi@student.ub.ac.id", "215150200111001")
        .await;

    for status in ["Menunggu Verifikasi", "Diterima", "Selesai", "Tidak Lolos"] {
        let id = ctx.pendaftaran(&user, status).await;
        let response = ctx.unduh(Some(&cookie), id).await;
        assert_eq!(response.status(), StatusCode::CONFLICT, "{status}");
        assert_eq!(
            json_body(response).await["message"],
            "Surat keterangan hanya tersedia untuk pendaftaran berstatus Aktif."
        );
    }
    assert_eq!(ctx.count_surat().await, 0);
    assert!(ctx.uploads.files().is_empty());
}

#[sqlx::test]
async fn only_the_owner_and_panitia_magang_can_download(db: PgPool) {
    let ctx = setup(db);
    let (pemilik, cookie_pemilik) = ctx.mahasiswa("budi@student.ub.ac.id", "1").await;
    let (_, cookie_lain) = ctx.mahasiswa("ani@student.ub.ac.id", "2").await;
    let cookie_tim = ctx.panitia_magang().await;
    let id = ctx.pendaftaran(&pemilik, "Aktif").await;

    assert_eq!(ctx.unduh(None, id).await.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        ctx.unduh(Some(&cookie_lain), id).await.status(),
        StatusCode::FORBIDDEN
    );
    // Panitia Magang may issue it too; the owner then gets the same file.
    let dari_tim = ctx.unduh_ok(&cookie_tim, id).await;
    assert_eq!(ctx.unduh_ok(&cookie_pemilik, id).await, dari_tim);
    assert_eq!(
        ctx.unduh(Some(&cookie_tim), 999_999).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test]
async fn without_a_signing_key_surat_are_unavailable(db: PgPool) {
    let ctx = setup_with_key(db, None);
    let (user, cookie) = ctx.mahasiswa("budi@student.ub.ac.id", "1").await;
    let id = ctx.pendaftaran(&user, "Aktif").await;

    assert_eq!(
        ctx.unduh(Some(&cookie), id).await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        ctx.verifikasi("1.abc").await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(ctx.count_surat().await, 0);
}

#[sqlx::test]
async fn the_code_in_the_qr_verifies_publicly(db: PgPool) {
    let ctx = setup(db);
    let (user, cookie) = ctx
        .mahasiswa("budi@student.ub.ac.id", "215150200111001")
        .await;
    let id = ctx.pendaftaran(&user, "Aktif").await;
    ctx.unduh_ok(&cookie, id).await;
    let kode = ctx.kode(id).await;

    // No cookie: whoever holds the paper must be able to check it.
    let response = ctx.verifikasi(&kode).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["nama"], "Budi Santoso");
    assert_eq!(body["nim"], "215150200111001");

    let (id_surat, tanda_tangan) = kode.split_once('.').unwrap();
    let mut palsu = tanda_tangan.to_string();
    let terakhir = if palsu.ends_with('A') { "B" } else { "A" };
    palsu.replace_range(palsu.len() - 1.., terakhir);
    for salah in [
        format!("{id_surat}.{palsu}"),
        format!("{}.{tanda_tangan}", id_surat.parse::<i32>().unwrap() + 1),
        "bukan-kode".to_string(),
    ] {
        assert_eq!(
            ctx.verifikasi(&salah).await.status(),
            StatusCode::NOT_FOUND,
            "{salah}"
        );
    }
}

#[sqlx::test]
async fn a_surat_edited_in_the_database_no_longer_verifies(db: PgPool) {
    let ctx = setup(db);
    let (user, cookie) = ctx
        .mahasiswa("budi@student.ub.ac.id", "215150200111001")
        .await;
    let id = ctx.pendaftaran(&user, "Aktif").await;
    let pdf = ctx.unduh_ok(&cookie, id).await;
    let kode = ctx.kode(id).await;

    sqlx::query("UPDATE surat_keterangan SET isi = replace(isi, 'Budi', 'Badu')")
        .execute(&ctx.db)
        .await
        .unwrap();

    assert_eq!(ctx.verifikasi(&kode).await.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        ctx.verifikasi_berkas(&pdf).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test]
async fn a_surat_signed_with_another_key_does_not_verify(db: PgPool) {
    let lain = setup_with_key(db.clone(), Some(KUNCI_LAIN));
    let (user, cookie) = lain.mahasiswa("budi@student.ub.ac.id", "1").await;
    let id = lain.pendaftaran(&user, "Aktif").await;
    let pdf = lain.unduh_ok(&cookie, id).await;
    let kode = lain.kode(id).await;

    let ctx = setup(db);
    assert_eq!(ctx.verifikasi(&kode).await.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        ctx.verifikasi_berkas(&pdf).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test]
async fn an_uploaded_pdf_verifies_only_if_it_is_the_issued_file(db: PgPool) {
    let ctx = setup(db);
    let (user, cookie) = ctx
        .mahasiswa("budi@student.ub.ac.id", "215150200111001")
        .await;
    let id = ctx.pendaftaran(&user, "Aktif").await;
    let pdf = ctx.unduh_ok(&cookie, id).await;

    let response = ctx.verifikasi_berkas(&pdf).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_body(response).await["nim"], "215150200111001");

    // One byte changed anywhere, as an edit would do.
    let mut diubah = pdf.clone();
    let tengah = diubah.len() / 2;
    diubah[tengah] ^= 1;
    assert_eq!(
        ctx.verifikasi_berkas(&diubah).await.status(),
        StatusCode::NOT_FOUND
    );

    let response = ctx.verifikasi_berkas(b"").await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        json_body(response).await["message"],
        "Pilih berkas PDF surat keterangan."
    );
}

#[sqlx::test]
async fn the_public_key_is_published(db: PgPool) {
    let ctx = setup(db);
    let response = ctx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/surat-keterangan/kunci-publik")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["algoritma"], "Ed25519");
    assert_eq!(body["kunci_publik"].as_str().unwrap().len(), 44);
}
