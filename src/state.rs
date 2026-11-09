use std::{path::Path, sync::Arc};

use sqlx::PgPool;

use crate::{
    config::Config,
    features::{
        auth::{GoogleClient, SessionManager},
        pendaftaran::TautanBerkas,
        surat_keterangan::PenandaSurat,
    },
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
    pub upload_dir: Arc<Path>,
    pub penanda_surat: Option<Arc<PenandaSurat>>,
    pub tautan_berkas: Arc<TautanBerkas>,
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
            upload_dir: config.upload_dir.as_path().into(),
            penanda_surat: config
                .surat_signing_key
                .as_deref()
                .map(|key| Arc::new(PenandaSurat::from_base64_seed(key))),
            tautan_berkas: Arc::new(TautanBerkas::new(&config.jwt_secret)),
        }
    }
}
