use serde::{Deserialize, Deserializer, Serialize, de::Error};
use sqlx::FromRow;

use crate::error::AppError;

const MAX_POSISI_LEN: usize = 255;

#[derive(Deserialize)]
pub struct PosisiQuery {
    pub p: Option<i32>,
    pub s: Option<String>,
    #[serde(default, deserialize_with = "one_or_zero")]
    pub is_full: Option<bool>,
    pub id_periode: Option<i32>,
}

fn one_or_zero<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<bool>, D::Error> {
    match String::deserialize(deserializer)?.as_str() {
        "1" => Ok(Some(true)),
        "0" => Ok(Some(false)),
        other => Err(D::Error::custom(format!(
            "is_full must be 1 or 0, got {other}"
        ))),
    }
}

#[derive(Debug, Serialize, FromRow)]
pub struct PerusahaanMitra {
    pub id_perusahaan: i32,
    pub nama_perusahaan: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct PosisiMagang {
    pub id_posisi: i32,
    pub posisi: String,
    pub deskripsi_persyaratan: String,
    pub kuota_total: i32,
    pub sisa_kuota: i32,
    pub id_periode: i32,
    #[sqlx(flatten)]
    pub perusahaan: PerusahaanMitra,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePosisi {
    pub id_perusahaan: i32,
    pub id_periode: i32,
    pub posisi: String,
    pub deskripsi_persyaratan: String,
    pub kuota_total: i32,
}

impl CreatePosisi {
    pub fn validate(mut self) -> Result<Self, AppError> {
        self.posisi = self.posisi.trim().to_string();
        if self.posisi.is_empty() || self.posisi.chars().count() > MAX_POSISI_LEN {
            return Err(AppError::Validation("Posisi harus berisi 1-255 karakter."));
        }

        self.deskripsi_persyaratan = self.deskripsi_persyaratan.trim().to_string();
        if self.deskripsi_persyaratan.is_empty() {
            return Err(AppError::Validation("Deskripsi persyaratan wajib diisi."));
        }

        if self.kuota_total < 1 {
            return Err(AppError::Validation("Kuota minimal 1."));
        }

        Ok(self)
    }
}
