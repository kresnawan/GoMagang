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

    #[cfg(test)]
    pub fn with_token_url(mut self, token_url: String) -> Self {
        self.token_url = token_url;
        self
    }

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

// The ID token comes straight from Google's token endpoint over TLS, so OpenID Connect
// allows skipping the signature check. The claims still have to be validated.
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

// A gmail.com address only says who the account belongs to. Whether it's really Panitia Magang is
// decided by the panitia_magang table when the session starts.
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
