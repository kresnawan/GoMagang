use std::sync::Arc;

use sqlx::PgPool;

use crate::{
    config::Config,
    features::auth::{GoogleClient, SessionManager},
};

// Shared dependencies for handlers go here.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub google: Arc<GoogleClient>,
    pub sessions: Arc<SessionManager>,
    pub frontend_url: Arc<str>,
    pub cookie_secure: bool,
    pub admin_secret: Option<Arc<str>>,
}

impl AppState {
    pub fn new(config: &Config, db: PgPool) -> Self {
        Self {
            db,
            google: Arc::new(GoogleClient::new(config)),
            sessions: Arc::new(SessionManager::new(config)),
            frontend_url: config.frontend_url.as_str().into(),
            cookie_secure: config.cookie_secure,
            admin_secret: config.admin_secret.as_deref().map(Arc::from),
        }
    }
}
