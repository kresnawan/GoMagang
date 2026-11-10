use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::error::AppError;

const MAX_NAMA_LEN: usize = 255;
const SKS_RANGE: RangeInclusive<i32> = 1..=24;

/// A mata kuliah in the konversi catalog. `deleted_at` is set once it has been hidden.
#[derive(Debug, Serialize, FromRow)]
pub struct MataKuliah {
    pub id_mk_konversi: i32,
    pub mk_asal: String,
    pub mk_konversi: String,
    pub sks: i32,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    pub termasuk_dihapus: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MataKuliahBaru {
    pub mk_asal: String,
    pub mk_konversi: String,
    pub sks: i32,
}

fn nama_valid(nama: &str) -> bool {
    !nama.is_empty() && nama.chars().count() <= MAX_NAMA_LEN
}

impl MataKuliahBaru {
    pub fn validate(mut self) -> Result<Self, AppError> {
        self.mk_asal = self.mk_asal.trim().to_string();
        self.mk_konversi = self.mk_konversi.trim().to_string();

        if !nama_valid(&self.mk_asal) {
            return Err(AppError::Validation("MK asal harus berisi 1-255 karakter."));
        }
        if !nama_valid(&self.mk_konversi) {
            return Err(AppError::Validation(
                "MK konversi harus berisi 1-255 karakter.",
            ));
        }
        if !SKS_RANGE.contains(&self.sks) {
            return Err(AppError::Validation("SKS harus berupa angka 1-24."));
        }

        Ok(self)
    }
}

/// A mata kuliah chosen for a pendaftaran, with the mata kuliah as it is now. The mata kuliah may
/// have been hidden since it was chosen.
#[derive(Debug, Serialize, FromRow)]
pub struct Penyetaraan {
    pub id_penyetaraan: i32,
    pub id_pendaftaran: i32,
    pub created_at: String,
    #[sqlx(flatten)]
    pub mata_kuliah: MataKuliah,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PenyetaraanBaru {
    pub id_mk_konversi: i32,
}

/// What became of a request to choose a mata kuliah.
#[derive(Debug)]
pub enum TambahResult {
    Ditambahkan(Penyetaraan),
    PendaftaranNotFound,
    NotOwner,
    NotAktif,
    MataKuliahNotFound,
    SudahAda,
}

/// What became of a request to drop a chosen mata kuliah.
#[derive(Debug)]
pub enum HapusResult {
    Dihapus,
    NotFound,
    NotOwner,
    NotAktif,
}
