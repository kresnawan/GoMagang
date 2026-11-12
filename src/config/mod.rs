pub mod cors;

use std::path::PathBuf;

use axum::http::HeaderValue;

const DEFAULT_CORS_ALLOWED_ORIGINS: &str = "http://localhost:5173";
const DEFAULT_GOOGLE_REDIRECT_URI: &str = "http://localhost:3000/auth/google";
const DEFAULT_FRONTEND_URL: &str = "http://localhost:5173";
const DEFAULT_UPLOAD_DIR: &str = "uploads";
const MIN_JWT_SECRET_LEN: usize = 32;
const MIN_ADMIN_SECRET_LEN: usize = 32;

pub struct Config {
    pub database_url: String,
    pub cors_allowed_origins: Vec<HeaderValue>,
    pub google_client_id: String,
    pub google_client_secret: String,
    pub google_redirect_uri: String,
    pub frontend_url: String,
    pub jwt_secret: String,
    pub admin_secret: Option<String>,
    pub cookie_secure: bool,
    pub upload_dir: PathBuf,
    /// Base64 of the 32-byte Ed25519 seed that signs surat keterangan. Without it, surat
    /// keterangan cannot be issued or verified.
    pub surat_signing_key: Option<String>,
    /// Without it, no email is sent, e.g. the notice to a mahasiswa when their status changes.
    pub smtp: Option<SmtpConfig>,
}

/// How the SMTP server is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtpTls {
    /// Plain connection upgraded with STARTTLS, usually on port 587.
    Starttls,
    /// TLS from the start, usually on port 465.
    Tls,
    /// No encryption, only for a local test server such as Mailpit.
    None,
}

pub struct SmtpConfig {
    pub host: String,
    pub port: Option<u16>,
    pub tls: SmtpTls,
    pub username: Option<String>,
    pub password: Option<String>,
    /// The sender, e.g. `Panitia Magang <magang@example.com>`.
    pub from: String,
}

impl SmtpConfig {
    /// Membaca konfigurasi SMTP dari environment.
    ///
    /// Mengembalikan `None` bila `SMTP_HOST` kosong atau tidak di-set; artinya pengiriman email
    /// dimatikan. Bila `SMTP_HOST` ada:
    /// - `SMTP_PORT` opsional; bila kosong, port mengikuti mode TLS.
    /// - `SMTP_TLS` bernilai `starttls` (default), `tls`, atau `none`.
    /// - `SMTP_USERNAME` dan `SMTP_PASSWORD` opsional.
    /// - `SMTP_FROM` wajib.
    ///
    /// # Panics
    /// Panic bila `SMTP_PORT` bukan angka, `SMTP_TLS` tidak dikenal, atau `SMTP_FROM` tidak di-set.
    fn from_env() -> Option<Self> {
        let host = env_nonempty("SMTP_HOST")?;
        let port = env_nonempty("SMTP_PORT").map(|port| {
            port.parse()
                .unwrap_or_else(|_| panic!("invalid SMTP_PORT: {port}"))
        });
        let tls = match env_nonempty("SMTP_TLS").as_deref() {
            None | Some("starttls") => SmtpTls::Starttls,
            Some("tls") => SmtpTls::Tls,
            Some("none") => SmtpTls::None,
            Some(other) => panic!("SMTP_TLS must be starttls, tls or none, not {other}"),
        };

        Some(Self {
            host,
            port,
            tls,
            username: env_nonempty("SMTP_USERNAME"),
            password: env_nonempty("SMTP_PASSWORD"),
            from: env_nonempty("SMTP_FROM").expect("SMTP_FROM must be set when SMTP_HOST is"),
        })
    }
}

/// Membaca variabel environment `name`, dengan nilai kosong dianggap sama dengan tidak di-set.
///
/// Dengan begitu, baris seperti `SMTP_HOST=` di file `.env` tidak menyalakan fitur secara tidak
/// sengaja.
fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

impl Config {
    /// Membaca seluruh konfigurasi aplikasi dari variabel environment.
    ///
    /// Variabel wajib: `DATABASE_URL`, `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, dan `JWT_SECRET`
    /// (minimal 32 byte). Variabel lain punya nilai default:
    /// - `CORS_ALLOWED_ORIGINS`: daftar origin dipisah koma, default `http://localhost:5173`.
    /// - `GOOGLE_REDIRECT_URI`: default `http://localhost:3000/auth/google`. Bila diawali
    ///   `https://`,
    ///   cookie sesi diberi atribut `Secure`.
    /// - `FRONTEND_URL`: default `http://localhost:5173`; garis miring di akhir dibuang.
    /// - `UPLOAD_DIR`: folder penyimpanan berkas, default `uploads`.
    /// - `ADMIN_SECRET` (minimal 32 byte bila di-set), `SURAT_SIGNING_KEY`, dan variabel `SMTP_*`
    ///   bersifat opsional.
    ///
    /// # Panics
    /// Panic bila variabel wajib tidak ada, secret terlalu pendek, atau ada origin CORS yang tidak
    /// valid,
    /// supaya kesalahan konfigurasi langsung terlihat saat server start.
    pub fn from_env() -> Self {
        let origins = std::env::var("CORS_ALLOWED_ORIGINS")
            .unwrap_or_else(|_| DEFAULT_CORS_ALLOWED_ORIGINS.to_string());
        let google_redirect_uri = std::env::var("GOOGLE_REDIRECT_URI")
            .unwrap_or_else(|_| DEFAULT_GOOGLE_REDIRECT_URI.to_string());
        let frontend_url =
            std::env::var("FRONTEND_URL").unwrap_or_else(|_| DEFAULT_FRONTEND_URL.to_string());
        let jwt_secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");
        assert!(
            jwt_secret.len() >= MIN_JWT_SECRET_LEN,
            "JWT_SECRET must be at least {MIN_JWT_SECRET_LEN} bytes"
        );

        let admin_secret = std::env::var("ADMIN_SECRET")
            .ok()
            .filter(|secret| !secret.is_empty());
        assert!(
            admin_secret
                .as_ref()
                .is_none_or(|secret| secret.len() >= MIN_ADMIN_SECRET_LEN),
            "ADMIN_SECRET must be at least {MIN_ADMIN_SECRET_LEN} bytes"
        );

        Self {
            database_url: std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
            cors_allowed_origins: parse_origins(&origins),
            google_client_id: std::env::var("GOOGLE_CLIENT_ID")
                .expect("GOOGLE_CLIENT_ID must be set"),
            google_client_secret: std::env::var("GOOGLE_CLIENT_SECRET")
                .expect("GOOGLE_CLIENT_SECRET must be set"),
            // Secure cookies are only sent over HTTPS, so they follow the backend's own scheme.
            cookie_secure: google_redirect_uri.starts_with("https://"),
            google_redirect_uri,
            frontend_url: frontend_url.trim_end_matches('/').to_string(),
            jwt_secret,
            admin_secret,
            upload_dir: std::env::var("UPLOAD_DIR")
                .unwrap_or_else(|_| DEFAULT_UPLOAD_DIR.to_string())
                .into(),
            surat_signing_key: std::env::var("SURAT_SIGNING_KEY")
                .ok()
                .filter(|key| !key.is_empty()),
            smtp: SmtpConfig::from_env(),
        }
    }
}

/// Memecah daftar origin CORS yang dipisah koma menjadi daftar `HeaderValue`.
///
/// Spasi di sekitar setiap origin dibuang dan entri kosong diabaikan, sehingga
/// `"http://a.test, http://b.test,"` tetap valid.
///
/// # Panics
/// Panic bila ada origin yang tidak bisa dijadikan header HTTP.
fn parse_origins(origins: &str) -> Vec<HeaderValue> {
    origins
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(|origin| {
            origin
                .parse()
                .unwrap_or_else(|_| panic!("invalid CORS origin: {origin}"))
        })
        .collect()
}
