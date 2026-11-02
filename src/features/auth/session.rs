use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::{extract::FromRequestParts, http::request::Parts};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};

use super::model::{SessionClaims, User};
use crate::{config::Config, error::AppError, state::AppState};

const SESSION_COOKIE: &str = "session";
const SESSION_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

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

    pub fn create_cookie(&self, user: User) -> Result<Cookie<'static>, AppError> {
        let iat = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AppError::Internal)?
            .as_secs();
        let claims = SessionClaims {
            user,
            iat,
            exp: iat + SESSION_TTL.as_secs(),
        };
        let token = encode(&Header::new(Algorithm::HS256), &claims, &self.encoding_key)
            .map_err(|_| AppError::Internal)?;

        Ok(Cookie::build((SESSION_COOKIE, token))
            .path("/")
            .http_only(true)
            .same_site(SameSite::Lax)
            .secure(self.cookie_secure)
            .max_age(SESSION_TTL.try_into().map_err(|_| AppError::Internal)?)
            .build())
    }

    pub fn removal_cookie(&self) -> Cookie<'static> {
        Cookie::build(SESSION_COOKIE).path("/").build()
    }

    fn decode(&self, token: &str) -> Result<User, AppError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_required_spec_claims(&["exp", "sub"]);

        decode::<SessionClaims>(token, &self.decoding_key, &validation)
            .map(|data| data.claims.user)
            .map_err(|_| AppError::Unauthorized)
    }
}

impl FromRequestParts<AppState> for User {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let jar = CookieJar::from_headers(&parts.headers);
        let cookie = jar.get(SESSION_COOKIE).ok_or(AppError::Unauthorized)?;

        state.sessions.decode(cookie.value())
    }
}
