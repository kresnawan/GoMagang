use reqwest::Url;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::error::AppError;

/// The length of the tautan_templat column.
const MAX_TAUTAN_LEN: usize = 255;

/// The template of the surat pernyataan DPA that is in use.
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
    /// Accepts only a full http or https link, so that it can be opened from the frontend as is.
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
