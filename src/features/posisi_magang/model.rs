use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::error::AppError;

const MAX_POSISI_LEN: usize = 255;

#[derive(Deserialize)]
pub struct PosisiQuery {
    pub p: Option<i32>,
    pub s: Option<String>,
    pub id_periode: Option<i32>,
    pub termasuk_dihapus: Option<bool>,
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
    /// Jumlah yang akan diterima. Hanya informasi: jumlah pendaftar tidak dibatasi olehnya.
    pub kuota: i32,
    pub id_periode: i32,
    #[sqlx(flatten)]
    pub perusahaan: PerusahaanMitra,
    /// Terisi setelah posisi disembunyikan, dalam WIB.
    pub deleted_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePosisi {
    pub id_perusahaan: i32,
    pub id_periode: i32,
    pub posisi: String,
    pub deskripsi_persyaratan: String,
    pub kuota: i32,
}

impl CreatePosisi {
    /// Memvalidasi dan merapikan body pembuatan posisi.
    ///
    /// Aturannya sama dengan perubahan posisi (lihat [`validate_isi`]). Nama posisi dan deskripsi
    /// yang
    /// sudah dirapikan disimpan kembali ke struct. `id_perusahaan` dan `id_periode` diperiksa
    /// keberadaannya saat disimpan ke database.
    pub fn validate(mut self) -> Result<Self, AppError> {
        (self.posisi, self.deskripsi_persyaratan) =
            validate_isi(&self.posisi, &self.deskripsi_persyaratan, self.kuota)?;
        Ok(self)
    }
}

/// Bagian posisi yang boleh diubah. Perusahaan dan periodenya tetap, karena pendaftaran pada posisi
/// itu terikat ke keduanya.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UbahPosisi {
    pub posisi: String,
    pub deskripsi_persyaratan: String,
    pub kuota: i32,
}

impl UbahPosisi {
    /// Memvalidasi dan merapikan body perubahan posisi, dengan aturan yang sama seperti pembuatan
    /// posisi
    /// (lihat [`validate_isi`]).
    pub fn validate(mut self) -> Result<Self, AppError> {
        (self.posisi, self.deskripsi_persyaratan) =
            validate_isi(&self.posisi, &self.deskripsi_persyaratan, self.kuota)?;
        Ok(self)
    }
}

/// Aturan bersama untuk membuat dan mengubah posisi.
///
/// - `posisi`: 1-255 karakter setelah spasi di awal dan akhir dibuang.
/// - `deskripsi_persyaratan`: tidak boleh kosong setelah dirapikan.
/// - `kuota`: minimal 1. Kuota hanya informasi jumlah yang akan diterima, bukan batas pendaftar.
///
/// Mengembalikan nama posisi dan deskripsi yang sudah dirapikan, atau [`AppError::Validation`].
fn validate_isi(
    posisi: &str,
    deskripsi_persyaratan: &str,
    kuota: i32,
) -> Result<(String, String), AppError> {
    let posisi = posisi.trim();
    if posisi.is_empty() || posisi.chars().count() > MAX_POSISI_LEN {
        return Err(AppError::Validation("Posisi harus berisi 1-255 karakter."));
    }

    let deskripsi_persyaratan = deskripsi_persyaratan.trim();
    if deskripsi_persyaratan.is_empty() {
        return Err(AppError::Validation("Deskripsi persyaratan wajib diisi."));
    }

    if kuota < 1 {
        return Err(AppError::Validation("Kuota minimal 1."));
    }

    Ok((posisi.to_string(), deskripsi_persyaratan.to_string()))
}
