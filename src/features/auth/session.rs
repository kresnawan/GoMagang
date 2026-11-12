use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::{extract::FromRequestParts, http::request::Parts};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use sha2::{Digest, Sha256};

use super::model::{AccessClaims, User};
use crate::{config::Config, error::AppError, state::AppState};

const ACCESS_TTL: Duration = Duration::from_secs(15 * 60);
pub const REFRESH_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
pub const REFRESH_GRACE: Duration = Duration::from_secs(10);
pub const REFRESH_COOKIE: &str = "refresh_token";

const ACCESS_COOKIE: &str = "access_token";
// Keeps the refresh token out of every request to the API.
const REFRESH_COOKIE_PATH: &str = "/auth";

pub struct SessionManager {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    cookie_secure: bool,
}

impl SessionManager {
    /// Membuat pengelola sesi dari konfigurasi.
    ///
    /// Kunci HMAC untuk menandatangani dan memverifikasi access token diturunkan dari `JWT_SECRET`.
    /// `cookie_secure` menentukan apakah cookie sesi hanya dikirim lewat HTTPS.
    pub fn new(config: &Config) -> Self {
        let secret = config.jwt_secret.as_bytes();

        Self {
            encoding_key: EncodingKey::from_secret(secret),
            decoding_key: DecodingKey::from_secret(secret),
            cookie_secure: config.cookie_secure,
        }
    }

    /// Membuat cookie `access_token` berisi JWT HS256 untuk `user`.
    ///
    /// JWT berlaku selama `ACCESS_TTL` (15 menit) dan memuat `sub` (id pengguna sebagai teks),
    /// `email`, `iat`, serta `exp`. Cookie dipasang di path `/`, sehingga ikut terkirim ke semua
    /// endpoint API, dengan atribut `HttpOnly` dan `SameSite=Lax`.
    ///
    /// Mengembalikan [`AppError::Internal`] bila jam sistem tidak valid atau token gagal dibuat.
    pub fn access_cookie(&self, user: &User) -> Result<Cookie<'static>, AppError> {
        let iat = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AppError::Internal)?
            .as_secs();
        let claims = AccessClaims {
            sub: user.id_pengguna.to_string(),
            email: user.email.clone(),
            iat,
            exp: iat + ACCESS_TTL.as_secs(),
        };
        let token = encode(&Header::new(Algorithm::HS256), &claims, &self.encoding_key)
            .map_err(|_| AppError::Internal)?;

        self.cookie(ACCESS_COOKIE, token, "/", ACCESS_TTL)
    }

    /// Membuat cookie `refresh_token` berisi `token` mentah dengan masa berlaku `max_age`.
    ///
    /// Cookie hanya dipasang di path `/auth`, sehingga refresh token tidak ikut terkirim pada
    /// request
    /// API biasa dan hanya terlihat oleh endpoint refresh dan logout. Di database, token ini
    /// disimpan
    /// dalam bentuk hash, bukan nilai mentahnya.
    pub fn refresh_cookie(
        &self,
        token: String,
        max_age: Duration,
    ) -> Result<Cookie<'static>, AppError> {
        self.cookie(REFRESH_COOKIE, token, REFRESH_COOKIE_PATH, max_age)
    }

    /// Menghapus cookie `access_token` dan `refresh_token` dari browser.
    ///
    /// Path setiap cookie harus sama dengan saat cookie dibuat; bila berbeda, browser tidak
    /// menganggapnya
    /// cookie yang sama dan tidak akan menghapusnya.
    pub fn clear_cookies(&self, jar: CookieJar) -> CookieJar {
        jar.remove(Cookie::build(ACCESS_COOKIE).path("/"))
            .remove(Cookie::build(REFRESH_COOKIE).path(REFRESH_COOKIE_PATH))
    }

    /// Membuat cookie sesi dengan atribut keamanan yang seragam.
    ///
    /// Semua cookie sesi bersifat `HttpOnly` (tidak bisa dibaca JavaScript), `SameSite=Lax`, dan
    /// `Secure` bila backend berjalan di HTTPS. Mengembalikan [`AppError::Internal`] bila `max_age`
    /// tidak bisa diubah menjadi durasi cookie.
    fn cookie(
        &self,
        name: &'static str,
        value: String,
        path: &'static str,
        max_age: Duration,
    ) -> Result<Cookie<'static>, AppError> {
        Ok(Cookie::build((name, value))
            .path(path)
            .http_only(true)
            .same_site(SameSite::Lax)
            .secure(self.cookie_secure)
            .max_age(max_age.try_into().map_err(|_| AppError::Internal)?)
            .build())
    }

    /// Memverifikasi dan membaca access token.
    ///
    /// Tanda tangan HS256 dan masa berlaku (`exp`) diperiksa, dan klaim `exp` serta `sub` wajib
    /// ada.
    /// Token yang rusak, kedaluwarsa, ditandatangani kunci lain, atau `sub`-nya bukan angka dijawab
    /// [`AppError::Unauthorized`].
    fn decode_access_token(&self, token: &str) -> Result<User, AppError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_required_spec_claims(&["exp", "sub"]);

        let claims = decode::<AccessClaims>(token, &self.decoding_key, &validation)
            .map_err(|_| AppError::Unauthorized)?
            .claims;

        claims.try_into().map_err(|_| AppError::Unauthorized)
    }
}

impl FromRequestParts<AppState> for User {
    type Rejection = AppError;

    /// Extractor Axum yang mengambil pengguna yang sedang login dari cookie `access_token`.
    ///
    /// Handler cukup menerima parameter `user: User` untuk mewajibkan login. Request tanpa cookie
    /// atau dengan token yang tidak valid langsung dijawab `401` sebelum handler dijalankan.
    /// Extractor ini tidak memeriksa peran; untuk itu dipakai middleware seperti
    /// `require_panitia_magang` atau `require_mahasiswa`.
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let jar = CookieJar::from_headers(&parts.headers);
        let cookie = jar.get(ACCESS_COOKIE).ok_or(AppError::Unauthorized)?;

        state.sessions.decode_access_token(cookie.value())
    }
}

/// Membuat token acak 32 byte (256 bit) dalam bentuk 64 karakter heksadesimal.
///
/// Dipakai untuk refresh token dan parameter `state` OAuth. Sumber acaknya adalah generator acak
/// yang aman secara kriptografis, sehingga token tidak bisa ditebak.
pub fn generate_token() -> String {
    rand::random::<[u8; 32]>()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Menghitung hash SHA-256 dari token.
///
/// Refresh token hanya disimpan dalam bentuk hash di database, sehingga kebocoran isi database
/// tidak langsung memberi akses ke sesi pengguna. SHA-256 tanpa salt sudah cukup di sini karena
/// token berasal dari 256 bit acak, bukan password yang bisa ditebak.
pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}
