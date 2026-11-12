use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jsonwebtoken::dangerous::insecure_decode_claims;
use reqwest::Url;

use super::model::{GoogleClaims, GoogleTokenResponse};
use crate::{config::Config, error::AppError};

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const SCOPE: &str = "openid email";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const ISSUERS: [&str; 2] = ["accounts.google.com", "https://accounts.google.com"];
const STUDENT_DOMAIN: &str = "student.ub.ac.id";
const GMAIL_DOMAIN: &str = "gmail.com";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AccountKind {
    Mahasiswa,
    PanitiaMagang,
}

pub struct GoogleClient {
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    token_url: String,
    http: reqwest::Client,
}

impl GoogleClient {
    /// Membuat klien OAuth Google dari konfigurasi.
    ///
    /// Klien HTTP diberi batas waktu `REQUEST_TIMEOUT` (5 detik), sehingga Google yang lambat tidak
    /// menahan proses login terlalu lama.
    ///
    /// # Panics
    /// Panic bila klien HTTP gagal dibuat, misalnya karena backend TLS tidak tersedia.
    pub fn new(config: &Config) -> Self {
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("failed to build HTTP client");

        Self {
            client_id: config.google_client_id.clone(),
            client_secret: config.google_client_secret.clone(),
            redirect_uri: config.google_redirect_uri.clone(),
            token_url: TOKEN_URL.to_string(),
            http,
        }
    }

    /// Mengganti URL token endpoint Google. Hanya untuk test, agar bisa diarahkan ke server tiruan.
    #[cfg(test)]
    pub fn with_token_url(mut self, token_url: String) -> Self {
        self.token_url = token_url;
        self
    }

    /// Menyusun URL halaman login Google yang dituju browser.
    ///
    /// URL memuat client ID, redirect URI, `response_type=code`, scope `openid email`, dan
    /// `prompt=select_account`, sehingga pengguna selalu bisa memilih akun. `state` adalah token
    /// acak
    /// yang juga disimpan di cookie, untuk mencegah CSRF pada callback.
    pub fn authorization_url(&self, state: &str) -> String {
        Url::parse_with_params(
            AUTH_URL,
            [
                ("client_id", self.client_id.as_str()),
                ("redirect_uri", self.redirect_uri.as_str()),
                ("response_type", "code"),
                ("scope", SCOPE),
                ("prompt", "select_account"),
                ("state", state),
            ],
        )
        .expect("AUTH_URL is a valid URL")
        .into()
    }

    /// Menukar authorization code dari callback Google dengan identitas pengguna.
    ///
    /// Code dikirim ke token endpoint Google bersama client secret. ID token yang diterima lalu
    /// divalidasi (lihat `decode_claims`) dan jenis akunnya ditentukan dari domain email (lihat
    /// `account_kind`).
    ///
    /// Error yang dikembalikan:
    /// - [`AppError::ServiceUnavailable`]: Google tidak bisa dihubungi atau jawabannya tidak
    ///   dikenali.
    /// - [`AppError::Unauthorized`]: code ditolak Google, atau ID token tidak valid.
    /// - [`AppError::Forbidden`]: email belum terverifikasi atau domainnya tidak diizinkan.
    pub async fn exchange_code(&self, code: &str) -> Result<(GoogleClaims, AccountKind), AppError> {
        let response = self
            .http
            .post(&self.token_url)
            .form(&[
                ("code", code),
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
                ("redirect_uri", &self.redirect_uri),
                ("grant_type", "authorization_code"),
            ])
            .send()
            .await
            .map_err(|err| {
                eprintln!("failed to reach Google token endpoint: {err}");
                AppError::ServiceUnavailable
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            eprintln!("Google rejected the authorization code: {status} {body}");
            return Err(AppError::Unauthorized);
        }

        let token: GoogleTokenResponse = response.json().await.map_err(|err| {
            eprintln!("invalid response from Google token endpoint: {err}");
            AppError::ServiceUnavailable
        })?;

        let claims = decode_claims(&token.id_token, &self.client_id)?;
        let kind = account_kind(&claims)?;

        Ok((claims, kind))
    }
}

/// Membaca dan memvalidasi klaim ID token dari Google.
///
/// Tanda tangan token tidak diperiksa. OpenID Connect mengizinkan hal ini karena token diterima
/// langsung dari token endpoint Google lewat TLS, bukan dari browser. Klaimnya tetap divalidasi:
/// `aud` harus sama dengan client ID aplikasi ini, `iss` harus Google, dan `exp` belum lewat.
/// Token yang gagal salah satu syarat dijawab [`AppError::Unauthorized`].
fn decode_claims(id_token: &str, client_id: &str) -> Result<GoogleClaims, AppError> {
    let claims: GoogleClaims =
        insecure_decode_claims(id_token).map_err(|_| AppError::Unauthorized)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AppError::Internal)?
        .as_secs();

    if claims.aud == client_id && ISSUERS.contains(&claims.iss.as_str()) && claims.exp > now {
        Ok(claims)
    } else {
        Err(AppError::Unauthorized)
    }
}

/// Menentukan jenis akun dari email Google yang sudah terverifikasi.
///
/// - Email `@student.ub.ac.id` dengan klaim `hd` (domain Google Workspace) yang sama dianggap
///   mahasiswa. Pengecekan `hd` memastikan akunnya benar-benar dikelola oleh domain kampus.
/// - Email `@gmail.com` tanpa `hd` dianggap calon Panitia Magang. Apakah benar Panitia Magang
///   baru diputuskan oleh tabel `panitia_magang` saat sesi dimulai.
/// - Selain itu, atau bila email belum terverifikasi, dijawab [`AppError::Forbidden`].
fn account_kind(claims: &GoogleClaims) -> Result<AccountKind, AppError> {
    if !claims.email_verified {
        return Err(AppError::Forbidden);
    }

    let email = claims.email.to_ascii_lowercase();
    let domain = email.rsplit_once('@').map(|(_, domain)| domain);

    match (domain, claims.hd.as_deref()) {
        (Some(STUDENT_DOMAIN), Some(STUDENT_DOMAIN)) => Ok(AccountKind::Mahasiswa),
        (Some(GMAIL_DOMAIN), None) => Ok(AccountKind::PanitiaMagang),
        _ => Err(AppError::Forbidden),
    }
}
