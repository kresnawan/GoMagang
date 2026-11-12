use axum::{
    body::Bytes,
    extract::{
        Multipart,
        multipart::{Field, MultipartError},
    },
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::{
    Date, OffsetDateTime, Time,
    macros::{format_description, offset},
};

use crate::{
    error::AppError,
    features::{mahasiswa::Mahasiswa, perusahaan::Perusahaan},
};

const MAX_BERKAS_SIZE: usize = 2 * 1024 * 1024;
/// Room for two full files and the multipart framing around them. A file that alone goes over
/// `MAX_BERKAS_SIZE` but fits in here still gets the per-file check.
pub const MAX_BODY_SIZE: usize = (2 + MAX_BERKAS_TAMBAHAN) * MAX_BERKAS_SIZE + 64 * 1024;
/// Portfolio and other extra files a mahasiswa may add, on top of the two required ones.
pub const MAX_BERKAS_TAMBAHAN: usize = 5;
pub const JENIS_BERKAS_TAMBAHAN: &str = "Portfolio dan Berkas Tambahan";
/// How many times a mahasiswa may register in one period, whatever happened to those registrations.
pub const MAX_PENDAFTARAN_PER_PERIODE: i64 = 2;
const DEFAULT_ROWS: i32 = 25;

const STATUS_PENDAFTARAN: [&str; 7] = [
    "Menunggu Verifikasi",
    "Lolos Seleksi Administrasi",
    "Diterima",
    "Aktif",
    "Selesai",
    "Tidak Lolos",
    "Batal/Mengundurkan Diri",
];

const MAX_LOKASI_LEN: usize = 255;
const MAX_DPM_LEN: usize = 255;
const TOO_LARGE: &str = "Ukuran berkas maksimal 2 MB.";
const PDF_SIGNATURE: &[u8] = b"%PDF-";

#[derive(Debug, Serialize, FromRow)]
pub struct Berkas {
    pub id_berkas: i32,
    pub jenis_berkas: String,
    pub nama_berkas: String,
    pub ukuran_berkas: i32,
}

#[derive(Debug, Serialize, FromRow)]
pub struct Pendaftaran {
    pub id_pendaftaran: i32,
    pub id_posisi: i32,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    #[sqlx(skip)]
    pub berkas: Vec<Berkas>,
}

#[derive(Debug, Deserialize)]
pub struct StatusQuery {
    pub id: i32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateStatus {
    pub status: String,
}

impl UpdateStatus {
    /// Memastikan status yang diminta adalah salah satu dari tujuh status pendaftaran yang dikenal.
    ///
    /// Status yang tidak dikenal dijawab [`AppError::Validation`]. Boleh tidaknya perpindahan dari
    /// status
    /// lama ke status ini diperiksa terpisah oleh [`panitia_magang_can_change`].
    pub fn validate(self) -> Result<Self, AppError> {
        if !STATUS_PENDAFTARAN.contains(&self.status.as_str()) {
            return Err(AppError::Validation("Status tidak dikenal."));
        }
        Ok(self)
    }
}

/// Mengecek apakah Panitia Magang boleh mengubah status dari `lama` ke `baru`.
///
/// Perubahan yang diizinkan:
/// - `Menunggu Verifikasi` → `Lolos Seleksi Administrasi` atau `Tidak Lolos`
/// - `Lolos Seleksi Administrasi` → `Diterima` atau `Tidak Lolos`
/// - `Aktif` → `Selesai`
///
/// `Diterima` → `Aktif` tidak termasuk, karena perubahan itu dilakukan mahasiswa lewat konfirmasi
/// penerimaan. Perubahan ke status yang sama juga ditolak.
pub fn panitia_magang_can_change(lama: &str, baru: &str) -> bool {
    matches!(
        (lama, baru),
        (
            "Menunggu Verifikasi",
            "Lolos Seleksi Administrasi" | "Tidak Lolos"
        ) | ("Lolos Seleksi Administrasi", "Diterima" | "Tidak Lolos")
            | ("Aktif", "Selesai")
    )
}

/// What became of a request to change a status.
#[derive(Debug, PartialEq)]
pub enum UpdateStatusResult {
    Updated { status_lama: String },
    NotFound,
    NotAllowed,
}

/// What became of a mahasiswa's request to accept a place.
#[derive(Debug, PartialEq)]
pub enum AcceptResult {
    Accepted,
    NotFound,
    NotOwner,
    NotDiterima,
    HasAktif,
    /// The 12 hours to confirm have passed.
    Kedaluwarsa,
}

/// One jadwal for every pendaftaran listed. The response sends back the same fields, as saved.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SimpanJadwal {
    pub id_pendaftaran: Vec<i32>,
    pub tanggal: String,
    pub jam: String,
    pub lokasi: String,
}

impl SimpanJadwal {
    /// Memvalidasi dan merapikan jadwal seleksi substansi.
    ///
    /// - `id_pendaftaran`: diurutkan, duplikat dibuang, dan tidak boleh kosong.
    /// - `tanggal`: format `YYYY-MM-DD` dan tidak boleh sebelum hari ini menurut WIB.
    /// - `jam`: format `HH:MM`.
    /// - `lokasi`: 1-255 karakter setelah spasi di awal dan akhir dibuang.
    ///
    /// Pelanggaran dijawab [`AppError::Validation`].
    pub fn validate(mut self) -> Result<Self, AppError> {
        rapikan_id_pendaftaran(&mut self.id_pendaftaran)?;

        let tanggal = Date::parse(&self.tanggal, format_description!("[year]-[month]-[day]"))
            .map_err(|_| AppError::Validation("Tanggal harus berformat YYYY-MM-DD."))?;
        if tanggal < OffsetDateTime::now_utc().to_offset(offset!(+7)).date() {
            return Err(AppError::Validation("Tanggal tidak boleh sudah lewat."));
        }

        Time::parse(&self.jam, format_description!("[hour]:[minute]"))
            .map_err(|_| AppError::Validation("Jam harus berformat HH:MM."))?;

        self.lokasi = self.lokasi.trim().to_string();
        if self.lokasi.is_empty() || self.lokasi.chars().count() > MAX_LOKASI_LEN {
            return Err(AppError::Validation("Lokasi harus berisi 1-255 karakter."));
        }

        Ok(self)
    }
}

/// One DPM for every pendaftaran listed. The response sends back the same fields, as saved.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SimpanPlotting {
    pub id_pendaftaran: Vec<i32>,
    pub nama_dpm: String,
    pub kontak_dpm: String,
}

impl SimpanPlotting {
    /// Memvalidasi dan merapikan data plotting DPM (Dosen Pembimbing Magang).
    ///
    /// - `id_pendaftaran`: diurutkan, duplikat dibuang, dan tidak boleh kosong.
    /// - `nama_dpm` dan `kontak_dpm`: 1-255 karakter setelah spasi di awal dan akhir dibuang.
    ///
    /// Pelanggaran dijawab [`AppError::Validation`].
    pub fn validate(mut self) -> Result<Self, AppError> {
        rapikan_id_pendaftaran(&mut self.id_pendaftaran)?;

        self.nama_dpm = self.nama_dpm.trim().to_string();
        if self.nama_dpm.is_empty() || self.nama_dpm.chars().count() > MAX_DPM_LEN {
            return Err(AppError::Validation(
                "Nama DPM harus berisi 1-255 karakter.",
            ));
        }

        self.kontak_dpm = self.kontak_dpm.trim().to_string();
        if self.kontak_dpm.is_empty() || self.kontak_dpm.chars().count() > MAX_DPM_LEN {
            return Err(AppError::Validation(
                "Kontak DPM harus berisi 1-255 karakter.",
            ));
        }

        Ok(self)
    }
}

/// Mengurutkan daftar id pendaftaran dan membuang duplikatnya.
///
/// Urutan yang tetap juga membuat penguncian baris di database selalu berurutan, sehingga dua
/// request tidak saling menunggu (deadlock). Daftar kosong dijawab [`AppError::Validation`].
pub(super) fn rapikan_id_pendaftaran(id_pendaftaran: &mut Vec<i32>) -> Result<(), AppError> {
    id_pendaftaran.sort_unstable();
    id_pendaftaran.dedup();
    if id_pendaftaran.is_empty() {
        return Err(AppError::Validation("Pilih minimal satu pendaftaran."));
    }
    Ok(())
}

/// What became of a request to save the data of a stage, such as a jadwal or a plotting.
#[derive(Debug, PartialEq)]
pub enum SimpanTahapResult {
    Saved,
    NotFound,
    NotAllowed,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    pub p: Option<i32>,
    pub r: Option<i32>,
    pub periode: Option<i32>,
    pub status: Option<i32>,
    pub perusahaan: Option<i32>,
    pub posisi: Option<i32>,
    pub s: Option<String>,
}

/// Which part of the list to send.
#[derive(Debug, Clone, Copy)]
pub struct Halaman {
    pub page: i32,
    pub rows: i32,
}

impl Halaman {
    /// Jumlah baris per halaman, untuk `LIMIT` di SQL.
    pub fn limit(self) -> i64 {
        i64::from(self.rows)
    }

    /// Jumlah baris yang dilewati sebelum halaman ini, untuk `OFFSET` di SQL. Halaman dimulai dari
    /// 1.
    pub fn offset(self) -> i64 {
        (i64::from(self.page) - 1) * i64::from(self.rows)
    }
}

/// Which pendaftaran to look at. What is left empty does not narrow them down.
#[derive(Debug, Default)]
pub struct FilterPendaftaran {
    pub id_pengguna: Option<i32>,
    pub id_pendaftaran: Option<i32>,
    pub id_periode: Option<i32>,
    pub id_status: Option<i32>,
    pub id_perusahaan: Option<i32>,
    pub nama: Option<String>,
    pub id_posisi: Option<i32>,
}

impl ListQuery {
    /// Halaman yang diminta lewat query `p` (nomor halaman, default 1) dan `r` (baris per halaman,
    /// default 25).
    ///
    /// Nilai di bawah 1 dijawab [`AppError::Validation`].
    pub fn halaman(&self) -> Result<Halaman, AppError> {
        let page = self.p.unwrap_or(1);
        let rows = self.r.unwrap_or(DEFAULT_ROWS);
        if page < 1 || rows < 1 {
            return Err(AppError::Validation("p dan r minimal 1."));
        }

        Ok(Halaman { page, rows })
    }

    /// Menyusun filter dari parameter query, dibatasi ke pendaftaran milik `id_pengguna` bila
    /// diisi.
    ///
    /// Mahasiswa selalu memanggil ini dengan id-nya sendiri, sehingga hanya bisa melihat
    /// pendaftarannya.
    /// Kotak pencarian `s` yang kosong atau hanya berisi spasi dianggap tidak mencari apa pun.
    pub fn filter(&self, id_pengguna: Option<i32>) -> FilterPendaftaran {
        FilterPendaftaran {
            id_pengguna,
            id_periode: self.periode,
            id_status: self.status,
            id_perusahaan: self.perusahaan,
            id_posisi: self.posisi,
            // An empty search box searches for nothing in particular.
            nama: self
                .s
                .as_deref()
                .map(str::trim)
                .filter(|nama| !nama.is_empty())
                .map(str::to_string),
            ..Default::default()
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DaftarPendaftaran {
    pub data: Vec<PendaftaranRingkas>,
    pub page: i32,
    pub rows: i32,
    pub total: i64,
    pub total_pages: i64,
}

impl DaftarPendaftaran {
    /// Menyusun respons daftar pendaftaran beserta informasi halaman.
    ///
    /// `total_pages` dihitung dari `total` dibagi `rows`, dibulatkan ke atas karena halaman
    /// terakhir
    /// bisa tidak penuh. Nilainya 0 bila tidak ada data.
    pub fn new(data: Vec<PendaftaranRingkas>, halaman: Halaman, total: i64) -> Self {
        let rows = i64::from(halaman.rows);
        Self {
            data,
            page: halaman.page,
            rows: halaman.rows,
            total,
            // Rounded up, as the last page may not be full.
            total_pages: (total + rows - 1) / rows,
        }
    }
}

/// What both the list and the detail say about a pendaftaran itself.
#[derive(Debug, Serialize, FromRow)]
pub struct InfoPendaftaran {
    pub id_pendaftaran: i32,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub id_posisi: i32,
    pub posisi: String,
    pub id_periode: i32,
    #[sqlx(flatten)]
    pub perusahaan: Perusahaan,
    /// While the status is Diterima: until when the mahasiswa may confirm, 12 hours after it was
    /// set. Past it the pendaftaran is cancelled. None in any other status.
    pub batas_konfirmasi: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct MahasiswaRingkas {
    pub id_pengguna: i32,
    pub nim: Option<String>,
    pub nama: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct PendaftaranRingkas {
    #[serde(flatten)]
    #[sqlx(flatten)]
    pub info: InfoPendaftaran,
    pub ada_jadwal_seleksi: bool,
    pub ada_plotting_dpm: bool,
    #[sqlx(flatten)]
    pub mahasiswa: MahasiswaRingkas,
}

#[derive(Debug, Serialize)]
pub struct MahasiswaDetail {
    pub id_pengguna: i32,
    #[serde(flatten)]
    pub data: Mahasiswa,
}

#[derive(Debug, Serialize, FromRow)]
pub struct LogStatus {
    pub id_log: i32,
    pub status_lama: Option<String>,
    pub status_baru: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct JadwalSeleksi {
    pub tanggal: String,
    pub jam: String,
    pub lokasi: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct SuratKeterangan {
    pub nama_berkas: String,
    pub ukuran_berkas: i32,
    pub nomor_surat: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct PlottingDpm {
    pub nama_dpm: String,
    pub kontak_dpm: String,
}

#[derive(Debug, Serialize)]
pub struct PendaftaranDetail {
    #[serde(flatten)]
    pub info: InfoPendaftaran,
    pub mahasiswa: MahasiswaDetail,
    pub berkas: Vec<Berkas>,
    pub log: Vec<LogStatus>,
    pub jadwal_seleksi: Option<JadwalSeleksi>,
    pub surat_keterangan: Option<SuratKeterangan>,
    pub plotting_dpm: Option<PlottingDpm>,
}

#[derive(Debug, FromRow)]
pub struct PosisiTujuan {
    pub periode_berlangsung: bool,
}

#[derive(Debug, FromRow)]
pub struct JumlahPendaftaran {
    /// Every registration in the target position's period, in any status.
    pub di_periode: i64,
    /// Registrations for the target position that have not reached a final status.
    pub berjalan_di_posisi: i64,
    pub aktif: i64,
}

#[derive(Debug, FromRow)]
pub struct LokasiBerkas {
    pub nama_berkas: String,
    pub jenis_berkas: String,
    pub id_pengguna: i32,
    pub nim: Option<String>,
}

/// A file about to be saved.
#[derive(Debug)]
pub struct BerkasBaru {
    pub jenis: &'static str,
    pub nama: String,
    pub isi: Bytes,
}

impl BerkasBaru {
    /// Menyiapkan berkas yang akan disimpan, dengan nama berkas `{id_pengguna}_{slug}_{acak}.pdf`.
    ///
    /// Nama berkas dibuat server, tidak pernah diambil dari unggahan, sehingga tidak bisa menunjuk
    /// ke
    /// luar folder upload. `id_pengguna` membuat pemilik berkas mudah dikenali, sedangkan 128 bit
    /// acak
    /// membuat nama berkas tidak bisa ditebak.
    fn new(jenis: &'static str, slug: &str, id_pengguna: i32, isi: Bytes) -> Self {
        let acak = rand::random::<u128>();
        Self {
            jenis,
            nama: format!("{id_pengguna}_{slug}_{acak:032x}.pdf"),
            isi,
        }
    }
}

#[derive(Debug)]
pub struct FormPendaftaran {
    pub id_posisi: i32,
    surat_dpa: Bytes,
    cv: Bytes,
    berkas_tambahan: Vec<Bytes>,
}

impl FormPendaftaran {
    /// Membaca form pendaftaran dari body multipart.
    ///
    /// Field yang diterima:
    /// - `id_posisi` (wajib): angka.
    /// - `surat_dpa` dan `cv` (wajib): masing-masing satu PDF maksimal 2 MB.
    /// - `berkas_tambahan` (opsional, boleh diulang): maksimal 5 PDF, masing-masing maksimal 2 MB.
    ///   Bagian kosong, seperti yang dikirim browser untuk input file yang tidak diisi, dilewati.
    ///
    /// Field yang tidak dikenal, atau field wajib yang dikirim lebih dari sekali, ditolak. Semua
    /// pelanggaran dijawab [`AppError::Validation`] dengan pesan untuk pengguna.
    pub async fn read(mut multipart: Multipart) -> Result<Self, AppError> {
        let mut id_posisi = None;
        let mut surat_dpa = None;
        let mut cv = None;
        let mut berkas_tambahan = Vec::new();

        while let Some(field) = multipart.next_field().await.map_err(read_error)? {
            match field.name() {
                Some("id_posisi") if id_posisi.is_none() => {
                    id_posisi = Some(read_id(field).await?);
                }
                Some("surat_dpa") if surat_dpa.is_none() => {
                    surat_dpa = Some(read_pdf(field).await?);
                }
                Some("cv") if cv.is_none() => cv = Some(read_pdf(field).await?),
                Some("berkas_tambahan") => {
                    // Optional, so an empty part, as a browser sends for an empty input, is skipped.
                    let isi = field.bytes().await.map_err(read_error)?;
                    if isi.is_empty() {
                        continue;
                    }
                    if berkas_tambahan.len() == MAX_BERKAS_TAMBAHAN {
                        return Err(AppError::Validation("Berkas tambahan maksimal 5 file."));
                    }
                    berkas_tambahan.push(check_pdf(isi)?);
                }
                _ => {
                    return Err(AppError::Validation(
                        "Field tidak dikenal atau dikirim lebih dari sekali.",
                    ));
                }
            }
        }

        Ok(Self {
            id_posisi: id_posisi.ok_or(AppError::Validation("Posisi wajib dipilih."))?,
            surat_dpa: surat_dpa.ok_or(BERKAS_WAJIB)?,
            cv: cv.ok_or(BERKAS_WAJIB)?,
            berkas_tambahan,
        })
    }

    /// Daftar berkas yang akan disimpan untuk pendaftaran ini: Surat Pernyataan DPA dan CV lebih
    /// dulu,
    /// lalu berkas tambahan sesuai urutan pengiriman.
    pub fn berkas(&self, id_pengguna: i32) -> Vec<BerkasBaru> {
        let mut berkas = vec![
            BerkasBaru::new(
                "Surat Pernyataan DPA",
                "surat-dpa",
                id_pengguna,
                self.surat_dpa.clone(),
            ),
            BerkasBaru::new("CV", "cv", id_pengguna, self.cv.clone()),
        ];
        berkas.extend(self.berkas_tambahan.iter().map(|isi| {
            BerkasBaru::new(
                JENIS_BERKAS_TAMBAHAN,
                "berkas-tambahan",
                id_pengguna,
                isi.clone(),
            )
        }));
        berkas
    }
}

const BERKAS_WAJIB: AppError = AppError::Validation("Surat Pernyataan DPA dan CV wajib diunggah.");

/// Membaca field `id_posisi` sebagai angka. Spasi di sekitarnya diabaikan.
async fn read_id(field: Field<'_>) -> Result<i32, AppError> {
    let text = field.text().await.map_err(read_error)?;
    text.trim()
        .parse()
        .map_err(|_| AppError::Validation("id_posisi harus berupa angka."))
}

/// Membaca field berkas wajib. Berkas kosong dianggap tidak diunggah, dan isinya diperiksa dengan
/// [`check_pdf`].
async fn read_pdf(field: Field<'_>) -> Result<Bytes, AppError> {
    let isi = field.bytes().await.map_err(read_error)?;
    if isi.is_empty() {
        return Err(BERKAS_WAJIB);
    }
    check_pdf(isi)
}

/// Memeriksa ukuran dan format berkas.
///
/// Ukuran maksimal 2 MB. Format dicek dari isinya, yaitu harus diawali tanda `%PDF-`, karena nama
/// berkas dan content type dikirim oleh klien dan bisa dipalsukan.
fn check_pdf(isi: Bytes) -> Result<Bytes, AppError> {
    if isi.len() > MAX_BERKAS_SIZE {
        return Err(AppError::Validation(TOO_LARGE));
    }
    // The file's name and content type come from the client, so only its content is checked.
    if !isi.starts_with(PDF_SIGNATURE) {
        return Err(AppError::Validation("Berkas harus berformat PDF."));
    }
    Ok(isi)
}

/// Menerjemahkan error pembacaan multipart: body yang melebihi batas ukuran dijawab "Ukuran berkas
/// maksimal 2 MB.", dan error lain dijawab "Format multipart tidak valid.".
fn read_error(err: MultipartError) -> AppError {
    if err.status() == StatusCode::PAYLOAD_TOO_LARGE {
        AppError::Validation(TOO_LARGE)
    } else {
        AppError::Validation("Format multipart tidak valid.")
    }
}
