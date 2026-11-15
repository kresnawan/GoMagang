use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::error::AppError;

const MAX_NAMA_LEN: usize = 255;

/// Perusahaan seperti yang ditampilkan fitur lain, tanpa status disembunyikan.
#[derive(Debug, Serialize, FromRow)]
pub struct Perusahaan {
    pub id_perusahaan: i32,
    pub nama_perusahaan: String,
}

/// Perusahaan seperti yang ditampilkan endpoint perusahaan. `deleted_at` terisi setelah perusahaan
/// disembunyikan.
#[derive(Debug, Serialize, FromRow)]
pub struct PerusahaanMitra {
    pub id_perusahaan: i32,
    pub nama_perusahaan: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    pub termasuk_dihapus: Option<bool>,
}

/// Body untuk membuat perusahaan maupun mengganti namanya.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamaPerusahaan {
    pub nama_perusahaan: String,
}

impl NamaPerusahaan {
    /// Merapikan dan memvalidasi nama perusahaan.
    ///
    /// Spasi di awal dan akhir dibuang, lalu nama harus berisi 1-255 karakter. Nama yang sudah
    /// dirapikan itu yang disimpan. Pelanggaran dijawab [`AppError::Validation`].
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

/// Hasil permintaan mengganti nama perusahaan.
#[derive(Debug)]
pub enum RenameResult {
    Renamed(PerusahaanMitra),
    NotFound,
    NameTaken,
}
