use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::error::AppError;

const MAX_NAMA_LEN: usize = 255;

#[derive(Debug, Serialize, FromRow)]
pub struct Perusahaan {
    pub id_perusahaan: i32,
    pub nama_perusahaan: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePerusahaan {
    pub nama_perusahaan: String,
}

impl CreatePerusahaan {
    pub fn validate(mut self) -> Result<Self, AppError> {
        self.nama_perusahaan = self.nama_perusahaan.trim().to_string();
        if self.nama_perusahaan.is_empty() || self.nama_perusahaan.chars().count() > MAX_NAMA_LEN {
            return Err(AppError::Validation(
                "Nama perusahaan harus berisi 1-255 karakter.",
            ));
        }

        Ok(self)
    }
}
