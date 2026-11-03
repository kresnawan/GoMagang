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
    pub fn new(config: &Config) -> Self {
        let secret = config.jwt_secret.as_bytes();

        Self {
            encoding_key: EncodingKey::from_secret(secret),
            decoding_key: DecodingKey::from_secret(secret),
            cookie_secure: config.cookie_secure,
        }
    }

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

    pub fn refresh_cookie(
        &self,
        token: String,
        max_age: Duration,
    ) -> Result<Cookie<'static>, AppError> {
        self.cookie(REFRESH_COOKIE, token, REFRESH_COOKIE_PATH, max_age)
    }

    pub fn clear_cookies(&self, jar: CookieJar) -> CookieJar {
        jar.remove(Cookie::build(ACCESS_COOKIE).path("/"))
            .remove(Cookie::build(REFRESH_COOKIE).path(REFRESH_COOKIE_PATH))
    }

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

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let jar = CookieJar::from_headers(&parts.headers);
        let cookie = jar.get(ACCESS_COOKIE).ok_or(AppError::Unauthorized)?;

        state.sessions.decode_access_token(cookie.value())
    }
}

pub fn generate_token() -> String {
    rand::random::<[u8; 32]>()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}
