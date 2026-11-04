use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Deserialize)]
pub struct GoogleCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
pub struct GoogleTokenResponse {
    pub id_token: String,
}

#[derive(Debug, Deserialize)]
pub struct GoogleClaims {
    pub sub: String,
    pub email: String,
    #[serde(default)]
    pub email_verified: bool,
    pub hd: Option<String>,
    pub aud: String,
    pub iss: String,
    pub exp: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Peran {
    Mahasiswa,
    TimMb,
}

#[derive(Debug, Serialize, FromRow)]
pub struct User {
    pub id_pengguna: i32,
    pub email: String,
}

#[derive(Serialize, Deserialize)]
pub struct AccessClaims {
    // JWT requires `sub` to be a string, so this holds `id_pengguna` as text.
    pub sub: String,
    pub email: String,
    pub iat: u64,
    pub exp: u64,
}

impl TryFrom<AccessClaims> for User {
    type Error = std::num::ParseIntError;

    fn try_from(claims: AccessClaims) -> Result<Self, Self::Error> {
        Ok(Self {
            id_pengguna: claims.sub.parse()?,
            email: claims.email,
        })
    }
}
