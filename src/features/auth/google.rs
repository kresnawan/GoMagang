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
const ALLOWED_DOMAIN: &str = "student.ub.ac.id";

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
                ("hd", ALLOWED_DOMAIN),
                ("prompt", "select_account"),
                ("state", state),
            ],
        )
        .expect("AUTH_URL is a valid URL")
        .into()
    }

    pub async fn exchange_code(&self, code: &str) -> Result<GoogleClaims, AppError> {
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
        check_allowed(&claims)?;

        Ok(claims)
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

fn check_allowed(claims: &GoogleClaims) -> Result<(), AppError> {
    let email_allowed = claims
        .email
        .to_ascii_lowercase()
        .strip_suffix(ALLOWED_DOMAIN)
        .is_some_and(|local| local.ends_with('@'));
    let hosted_domain_allowed = claims.hd.as_deref() == Some(ALLOWED_DOMAIN);

    if claims.email_verified && email_allowed && hosted_domain_allowed {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}
