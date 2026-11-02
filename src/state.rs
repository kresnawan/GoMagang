use std::sync::Arc;

use crate::{
    config::Config,
    features::auth::{GoogleClient, SessionManager},
};

// Shared dependencies for handlers, e.g. the database pool, go here.
#[derive(Clone)]
pub struct AppState {
    pub google: Arc<GoogleClient>,
    pub sessions: Arc<SessionManager>,
    pub frontend_url: Arc<str>,
    pub cookie_secure: bool,
}

impl AppState {
    pub fn new(config: &Config) -> Self {
        Self {
            google: Arc::new(GoogleClient::new(config)),
            sessions: Arc::new(SessionManager::new(config)),
            frontend_url: config.frontend_url.as_str().into(),
            cookie_secure: config.cookie_secure,
        }
    }
}
