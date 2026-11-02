use serde::{Deserialize, Serialize};

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
    pub name: Option<String>,
    pub hd: Option<String>,
    pub aud: String,
    pub iss: String,
    pub exp: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct User {
    pub sub: String,
    pub email: String,
    pub name: Option<String>,
}

impl From<GoogleClaims> for User {
    fn from(claims: GoogleClaims) -> Self {
        Self {
            sub: claims.sub,
            email: claims.email,
            name: claims.name,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct SessionClaims {
    #[serde(flatten)]
    pub user: User,
    pub iat: u64,
    pub exp: u64,
}
