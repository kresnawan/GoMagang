use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Dinaikkan setiap kali field yang dicetak berubah, sehingga surat lama tetap terbaca sesuai
/// versinya.
pub const VERSI_ISI: u8 = 1;

/// Ukuran PDF terbesar yang diterima untuk verifikasi. Surat yang diterbitkan hanya beberapa
/// kilobyte.
pub const MAX_BERKAS_VERIFIKASI: usize = 2 * 1024 * 1024;

/// Semua yang dicetak di surat, dibekukan saat surat terbit. Isi ini diubah ke JSON sekali, dan
/// teks persis itulah yang disimpan dan ditandatangani, sehingga perubahan data mahasiswa, posisi,
/// atau templat setelahnya tidak mengubah surat yang sudah terbit.
#[derive(Debug, Serialize, Deserialize)]
pub struct IsiSurat {
    pub versi: u8,
    pub nomor_surat: String,
    /// `YYYY-MM-DD`, dalam WIB
    pub tanggal_surat: String,
    pub nama: String,
    pub nim: String,
    pub program_studi: String,
    pub posisi: String,
    pub perusahaan: String,
    pub alamat_perusahaan: String,
    pub program: String,
    /// YYYY-MM-DD
    pub tanggal_mulai: String,
    /// YYYY-MM-DD
    pub tanggal_selesai: String,
    pub nama_pejabat: String,
    pub nip_pejabat: String,
    pub jabatan_pejabat: String,
}

/// Data pendaftaran yang dibutuhkan surat, beserta pemiliknya.
#[derive(Debug, FromRow)]
pub struct DataPendaftaran {
    pub id_pengguna: i32,
    pub status: String,
    pub nama: Option<String>,
    pub nim: Option<String>,
    pub program_studi: Option<String>,
    pub posisi: String,
    pub perusahaan: String,
}

#[derive(Debug, FromRow)]
pub struct SuratTersimpan {
    pub id_surat: i32,
    pub nama_berkas: String,
    pub isi: String,
    pub tanda_tangan: String,
}

pub struct SuratBaru {
    pub id_surat: i32,
    pub id_pendaftaran: i32,
    pub nama_berkas: String,
    pub ukuran_berkas: i32,
    pub nomor_surat: String,
    pub isi: String,
    pub tanda_tangan: String,
    pub hash_berkas: String,
}

#[derive(Debug, Deserialize)]
pub struct KodeQuery {
    pub kode: String,
}

#[derive(Debug, Serialize)]
pub struct KunciPublik {
    pub algoritma: &'static str,
    pub kunci_publik: String,
}
