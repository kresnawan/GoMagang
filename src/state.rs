use std::{path::Path, sync::Arc};

use sqlx::PgPool;

use crate::{
    config::Config,
    email::PengirimEmail,
    features::{
        auth::{GoogleClient, SessionManager},
        pendaftaran::TautanBerkas,
        surat_keterangan::PenandaSurat,
    },
};

/// Dependency bersama yang dibutuhkan handler. Murah untuk di-clone karena isinya berupa `Arc` atau
/// pool.
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
    pub email: Arc<PengirimEmail>,
}

impl AppState {
    /// Membuat state bersama untuk semua handler dari konfigurasi dan pool database.
    ///
    /// Objek yang mahal atau perlu dibagi (klien Google, pengelola sesi, penanda tangan surat,
    /// penanda tautan berkas, pengirim email) dibungkus `Arc`, sehingga `AppState` murah untuk
    /// di-clone
    /// pada setiap request. `penanda_surat` bernilai `None` bila `SURAT_SIGNING_KEY` tidak di-set,
    /// dan
    /// fitur surat keterangan kemudian menjawab `503`.
    ///
    /// # Panics
    /// Panic bila `SURAT_SIGNING_KEY` atau konfigurasi SMTP formatnya tidak valid, agar kesalahan
    /// konfigurasi ketahuan saat start, bukan saat dipakai.
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
            email: Arc::new(PengirimEmail::new(config.smtp.as_ref())),
        }
    }
}
