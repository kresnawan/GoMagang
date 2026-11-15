use reqwest::Url;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::error::AppError;

/// Panjang kolom `tautan_templat`.
const MAX_TAUTAN_LEN: usize = 255;

/// Templat surat pernyataan DPA yang sedang berlaku.
#[derive(Debug, Serialize, FromRow)]
pub struct TemplatSpDpa {
    pub id_templat: i32,
    pub tautan_templat: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimpanTemplat {
    pub tautan_templat: String,
}

impl SimpanTemplat {
    /// Memvalidasi tautan templat.
    ///
    /// Spasi di awal dan akhir dibuang, lalu tautan harus berisi 1-255 karakter (sesuai panjang
    /// kolom)
    /// dan berupa URL lengkap `http://` atau `https://` dengan host, tanpa spasi. Dengan begitu
    /// tautan
    /// bisa langsung dibuka dari frontend. Pelanggaran dijawab [`AppError::Validation`].
    pub fn validate(mut self) -> Result<Self, AppError> {
        self.tautan_templat = self.tautan_templat.trim().to_string();

        if self.tautan_templat.is_empty() || self.tautan_templat.chars().count() > MAX_TAUTAN_LEN {
            return Err(AppError::Validation(
                "Tautan templat harus berisi 1-255 karakter.",
            ));
        }

        let valid = Url::parse(&self.tautan_templat).is_ok_and(|url| {
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some_and(|host| !host.is_empty())
        });
        if !valid || self.tautan_templat.chars().any(char::is_whitespace) {
            return Err(AppError::Validation(
                "Tautan templat harus berupa URL lengkap yang diawali http:// atau https://.",
            ));
        }

        Ok(self)
    }
}
