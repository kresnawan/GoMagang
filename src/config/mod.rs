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
}

impl Config {
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
        }
    }
}

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
