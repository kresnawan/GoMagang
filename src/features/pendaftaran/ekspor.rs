//! Mengekspor pendaftaran pilihan Panitia Magang ke sebuah berkas. Untuk saat ini format yang
//! tersedia hanya Excel (.xlsx).

use rust_xlsxwriter::{Color, ExcelDateTime, Format, FormatAlign, Url, Workbook, XlsxError};
use serde::Deserialize;
use sqlx::{FromRow, PgPool};
use time::{OffsetDateTime, macros::format_description, macros::offset};

use super::model::rapikan_id_pendaftaran;
use crate::error::AppError;

/// Cukup untuk semua pendaftaran dalam satu periode, sekaligus mencegah satu request membangun
/// berkas yang sangat besar.
pub const MAX_EKSPOR: usize = 10_000;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormatEkspor {
    Xlsx,
}

impl FormatEkspor {
    /// Content type HTTP untuk format ekspor, dipakai di header `Content-Type` respons.
    pub fn content_type(self) -> &'static str {
        match self {
            Self::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        }
    }

    /// Ekstensi nama berkas untuk format ekspor, tanpa titik.
    fn ekstensi(self) -> &'static str {
        match self {
            Self::Xlsx => "xlsx",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ekspor {
    pub format: FormatEkspor,
    pub id_pendaftaran: Vec<i32>,
}

impl Ekspor {
    /// Memvalidasi permintaan ekspor.
    ///
    /// Daftar id diurutkan, duplikatnya dibuang, dan tidak boleh kosong. Jumlahnya maksimal
    /// [`MAX_EKSPOR`] (10.000), cukup untuk semua pendaftaran satu periode sekaligus mencegah satu
    /// request membangun berkas yang sangat besar.
    pub fn validate(mut self) -> Result<Self, AppError> {
        rapikan_id_pendaftaran(&mut self.id_pendaftaran)?;
        if self.id_pendaftaran.len() > MAX_EKSPOR {
            return Err(AppError::Validation(
                "Maksimal 10000 pendaftaran dalam satu ekspor.",
            ));
        }
        Ok(self)
    }
}

/// Satu baris di berkas ekspor. Waktunya dalam WIB, sesuai yang tersimpan di database.
#[derive(Debug, FromRow)]
pub struct BarisEkspor {
    pub id_pendaftaran: i32,
    pub nama: Option<String>,
    pub nim: Option<String>,
    pub email: String,
    pub jenis_kelamin: Option<String>,
    pub program_studi: Option<String>,
    pub semester: Option<i16>,
    pub ipk: Option<f64>,
    pub no_hp: Option<String>,
    pub posisi: String,
    pub nama_perusahaan: String,
    pub catatan_periode: Option<String>,
    /// YYYY-MM-DD
    pub mulai_periode: String,
    /// YYYY-MM-DD
    pub tutup_periode: String,
    pub status: String,
    /// YYYY-MM-DD HH:MM:SS
    pub created_at: String,
    /// YYYY-MM-DD HH:MM:SS
    pub updated_at: String,
    /// YYYY-MM-DD
    pub tanggal_seleksi: Option<String>,
    /// HH:MM
    pub jam_seleksi: Option<String>,
    pub lokasi_seleksi: Option<String>,
    pub nama_dpm: Option<String>,
    pub kontak_dpm: Option<String>,
    pub nomor_surat: Option<String>,
    /// Diisi oleh `lampirkan_berkas`, sesuai urutan berkas diunggah.
    #[sqlx(skip)]
    pub berkas: Vec<TautanEkspor>,
}

/// Tautan publik ke satu berkas pendaftaran.
#[derive(Debug, Default)]
pub struct TautanEkspor {
    pub jenis: String,
    pub url: String,
}

#[derive(Debug, FromRow)]
pub struct BerkasEkspor {
    pub id_pendaftaran: i32,
    pub id_berkas: i32,
    pub jenis_berkas: String,
}

/// Informasi untuk pembaca di lembar Keterangan, misalnya sampai kapan tautan berkas berlaku.
pub struct InfoEkspor {
    pub diekspor: OffsetDateTime,
    pub tautan_berlaku_sampai: OffsetDateTime,
}

/// Data setiap pendaftaran yang diekspor, dari yang paling baru seperti di daftar pendaftaran.
///
/// Id yang tidak dikenal tidak disertakan, sehingga pemanggil bisa membandingkan jumlahnya untuk
/// mendeteksi id yang salah. Data jadwal seleksi, plotting DPM, dan nomor surat ikut diambil bila
/// ada.
/// Beberapa kolom di-cast: NIM dan jenis kelamin menjadi teks agar spasi `CHAR` hilang, dan IPK
/// menjadi bilangan desimal. Waktu ditulis dalam WIB tanpa offset, sesuai format sel Excel.
pub async fn find_baris(
    db: &PgPool,
    id_pendaftaran: &[i32],
) -> Result<Vec<BarisEkspor>, sqlx::Error> {
    sqlx::query_as(
        "SELECT p.id_pendaftaran, m.nama, m.nim::text AS nim, pg.email, m.jenis_kelamin::text AS jenis_kelamin,
                m.program_studi, m.semester, m.ipk::float8 AS ipk, m.no_hp,
                pm.posisi, pr.nama_perusahaan,
                pp.catatan AS catatan_periode,
                to_char(pp.waktu_mulai, 'YYYY-MM-DD') AS mulai_periode,
                to_char(pp.waktu_tutup, 'YYYY-MM-DD') AS tutup_periode,
                s.status,
                to_char(p.created_at, 'YYYY-MM-DD HH24:MI:SS') AS created_at,
                to_char(p.updated_at, 'YYYY-MM-DD HH24:MI:SS') AS updated_at,
                to_char(j.tanggal, 'YYYY-MM-DD') AS tanggal_seleksi,
                to_char(j.jam, 'HH24:MI') AS jam_seleksi,
                j.lokasi AS lokasi_seleksi,
                d.nama_dpm, d.kontak_dpm,
                sk.nomor_surat
         FROM pendaftaran p
         JOIN status_pendaftaran s ON s.id_status = p.id_status
         JOIN mahasiswa m ON m.id_pengguna = p.id_pengguna
         JOIN pengguna pg ON pg.id_pengguna = p.id_pengguna
         JOIN posisi_magang pm ON pm.id_posisi = p.id_posisi
         JOIN perusahaan_mitra pr ON pr.id_perusahaan = pm.id_perusahaan
         JOIN periode_pendaftaran pp ON pp.id_periode = pm.id_periode
         LEFT JOIN jadwal_seleksi_substansi j ON j.id_pendaftaran = p.id_pendaftaran
         LEFT JOIN plotting_dpm d ON d.id_pendaftaran = p.id_pendaftaran
         LEFT JOIN surat_keterangan sk ON sk.id_pendaftaran = p.id_pendaftaran
         WHERE p.id_pendaftaran = ANY($1)
         ORDER BY p.created_at DESC, p.id_pendaftaran DESC",
    )
    .bind(id_pendaftaran)
    .fetch_all(db)
    .await
}

/// Berkas-berkas milik pendaftaran yang diekspor, sesuai urutan diunggah.
pub async fn find_berkas(
    db: &PgPool,
    id_pendaftaran: &[i32],
) -> Result<Vec<BerkasEkspor>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id_pendaftaran, id_berkas, jenis_berkas
         FROM berkas_pendaftaran
         WHERE id_pendaftaran = ANY($1)
         ORDER BY id_berkas",
    )
    .bind(id_pendaftaran)
    .fetch_all(db)
    .await
}

/// Memasangkan tautan setiap berkas ke baris pendaftarannya.
///
/// Tautan dibuat oleh `url` dari `id_berkas`, sehingga modul ini tidak perlu tahu cara tautan
/// publik ditandatangani. Urutan berkas dalam satu baris mengikuti urutan `berkas`.
pub fn lampirkan_berkas(
    semua: &mut [BarisEkspor],
    berkas: Vec<BerkasEkspor>,
    url: impl Fn(i32) -> String,
) {
    for file in berkas {
        if let Some(baris) = semua
            .iter_mut()
            .find(|baris| baris.id_pendaftaran == file.id_pendaftaran)
        {
            baris.berkas.push(TautanEkspor {
                jenis: file.jenis_berkas,
                url: url(file.id_berkas),
            });
        }
    }
}

/// Nama berkas hasil ekspor berdasarkan awalan dan waktu sekarang dalam WIB, misalnya
/// `pendaftar-20261109-1430.xlsx` untuk awalan `pendaftar`.
pub fn nama_berkas(awalan: &str, format: FormatEkspor) -> String {
    let sekarang = OffsetDateTime::now_utc().to_offset(offset!(+7));
    let waktu = sekarang
        .format(format_description!("[year][month][day]-[hour][minute]"))
        .unwrap_or_default();
    format!("{awalan}-{waktu}.{}", format.ekstensi())
}

const JUDUL_KOLOM: [&str; 21] = [
    "No",
    "Nama",
    "NIM",
    "Email",
    "Jenis Kelamin",
    "Program Studi",
    "Semester",
    "IPK",
    "No. HP",
    "Posisi",
    "Perusahaan",
    "Periode",
    "Status",
    "Tanggal Daftar",
    "Terakhir Diubah",
    "Tanggal Seleksi Substansi",
    "Jam Seleksi Substansi",
    "Lokasi Seleksi Substansi",
    "Nama DPM",
    "Kontak DPM",
    "Nomor Surat Keterangan",
];

const BULAN: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des",
];

/// Menulis tanggal `YYYY-MM-DD` dalam format pendek bahasa Indonesia, misalnya `2026-10-01` menjadi
/// `1 Okt 2026`. Teks yang bukan tanggal valid dikembalikan apa adanya.
fn tanggal_pendek(tanggal: &str) -> String {
    let mut bagian = tanggal.splitn(3, '-');
    let (Some(tahun), Some(bulan), Some(hari)) = (bagian.next(), bagian.next(), bagian.next())
    else {
        return tanggal.to_string();
    };
    match (bulan.parse::<usize>(), hari.parse::<u8>()) {
        (Ok(bulan @ 1..=12), Ok(hari)) => format!("{hari} {} {tahun}", BULAN[bulan - 1]),
        _ => tanggal.to_string(),
    }
}

/// Menulis waktu dalam WIB, misalnya `9 Des 2026 14.30 WIB`. Dipakai di lembar Keterangan.
pub(super) fn waktu_wib(waktu: OffsetDateTime) -> String {
    let waktu = waktu.to_offset(offset!(+7));
    format!(
        "{} {} {} {:02}.{:02} WIB",
        waktu.day(),
        BULAN[usize::from(u8::from(waktu.month())) - 1],
        waktu.year(),
        waktu.hour(),
        waktu.minute()
    )
}

/// Satu kolom tautan untuk sebuah jenis berkas, atau untuk berkas ke-n dari jenis yang bisa
/// dimiliki lebih dari satu oleh sebuah pendaftaran, misalnya berkas tambahan.
#[derive(Debug, PartialEq)]
struct KolomBerkas<'a> {
    jenis: &'a str,
    /// 1 untuk berkas pertama jenis ini dalam satu baris, 2 untuk yang kedua, dan seterusnya.
    ke: usize,
}

/// Menentukan kolom untuk setiap berkas dalam satu baris.
///
/// Setiap berkas dipasangkan dengan jenisnya dan urutan ke berapa berkas jenis itu muncul di baris
/// tersebut. Misalnya, berkas tambahan kedua menjadi `(Portfolio dan Berkas Tambahan, 2)`.
fn urutan_berkas(baris: &BarisEkspor) -> impl Iterator<Item = (KolomBerkas<'_>, &TautanEkspor)> {
    let mut sudah: Vec<&str> = Vec::new();
    baris.berkas.iter().map(move |tautan| {
        sudah.push(&tautan.jenis);
        let ke = sudah.iter().filter(|jenis| **jenis == tautan.jenis).count();
        (
            KolomBerkas {
                jenis: &tautan.jenis,
                ke,
            },
            tautan,
        )
    })
}

/// Menyusun kolom-kolom tautan berkas beserta judulnya.
///
/// Kolom diurutkan berdasarkan jenis berkas sesuai urutan pertama kali muncul, lalu berdasarkan
/// urutan ke berapa. Kolom suatu jenis hanya diberi nomor (`URL X 1`, `URL X 2`) bila ada baris
/// yang
/// punya lebih dari satu berkas jenis itu; selain itu judulnya cukup `URL X`. Kolom bernomor dari
/// jenis yang sama selalu berdampingan.
fn kolom_berkas(semua: &[BarisEkspor]) -> (Vec<KolomBerkas<'_>>, Vec<String>) {
    let mut kolom: Vec<KolomBerkas> = Vec::new();
    for baris in semua {
        for (item, _) in urutan_berkas(baris) {
            if !kolom.contains(&item) {
                kolom.push(item);
            }
        }
    }
    // A kind with a second file is numbered, so its columns are kept together and in order.
    let urutan_jenis: Vec<&str> = kolom.iter().fold(Vec::new(), |mut urutan, item| {
        if !urutan.contains(&item.jenis) {
            urutan.push(item.jenis);
        }
        urutan
    });
    kolom.sort_by_key(|item| {
        let jenis = urutan_jenis.iter().position(|jenis| *jenis == item.jenis);
        (jenis, item.ke)
    });
    let judul = kolom
        .iter()
        .map(|item| {
            let banyak = kolom
                .iter()
                .any(|lain| lain.jenis == item.jenis && lain.ke > 1);
            if banyak {
                format!("URL {} {}", item.jenis, item.ke)
            } else {
                format!("URL {}", item.jenis)
            }
        })
        .collect();
    (kolom, judul)
}

/// Label periode untuk kolom Periode, misalnya `Ganjil 2026/2027 (1 Okt 2026 – 30 Nov 2026)`.
///
/// Bila periode tidak punya catatan, yang ditulis hanya rentang tanggalnya.
pub(super) fn label_periode(baris: &BarisEkspor) -> String {
    let rentang = format!(
        "{} – {}",
        tanggal_pendek(&baris.mulai_periode),
        tanggal_pendek(&baris.tutup_periode)
    );
    match baris.catatan_periode.as_deref().map(str::trim) {
        Some(catatan) if !catatan.is_empty() => format!("{catatan} ({rentang})"),
        _ => rentang,
    }
}

/// Menulis kode jenis kelamin dalam bentuk lengkap: `L` menjadi `Laki-laki` dan `P` menjadi
/// `Perempuan`. Kode lain dikembalikan apa adanya.
fn jenis_kelamin(kode: &str) -> &str {
    match kode {
        "L" => "Laki-laki",
        "P" => "Perempuan",
        other => other,
    }
}

/// Menyusun workbook Excel hasil ekspor.
///
/// Lembar **Pendaftar** berisi satu baris per pendaftaran: kolom-kolom tetap (data diri, posisi,
/// periode, status, waktu, seleksi, DPM, nomor surat), lalu kolom tautan berkas (lihat
/// [`kolom_berkas`]). Baris judul dibekukan dan diberi autofilter. Angka, tanggal, dan jam ditulis
/// sebagai nilai Excel dengan format yang sesuai.
///
/// Lembar **Keterangan** berisi waktu ekspor, jumlah pendaftar, dan masa berlaku tautan berkas.
///
/// Teks selalu ditulis sebagai teks, sehingga nilai dari mahasiswa seperti `=1+1` tidak pernah
/// dijalankan sebagai rumus (mencegah formula injection).
pub fn buat_xlsx(semua: &[BarisEkspor], info: &InfoEkspor) -> Result<Vec<u8>, XlsxError> {
    let judul = Format::new()
        .set_bold()
        .set_font_color(Color::White)
        .set_background_color(Color::RGB(0x202F66))
        .set_align(FormatAlign::VerticalCenter);
    let teks = Format::new();
    let angka = Format::new().set_align(FormatAlign::Left);
    let ipk = Format::new()
        .set_num_format("0.00")
        .set_align(FormatAlign::Left);
    let waktu = Format::new()
        .set_num_format("dd/mm/yyyy hh:mm")
        .set_align(FormatAlign::Left);
    let tanggal = Format::new()
        .set_num_format("dd/mm/yyyy")
        .set_align(FormatAlign::Left);
    let jam = Format::new()
        .set_num_format("hh:mm")
        .set_align(FormatAlign::Left);

    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.set_name("Pendaftar")?;

    for (kolom, isi) in JUDUL_KOLOM.iter().enumerate() {
        sheet.write_string_with_format(0, kolom as u16, *isi, &judul)?;
    }
    // One column of links per kind of file, after the fixed ones.
    let (kolom_url, judul_url) = kolom_berkas(semua);
    let kolom_berkas = JUDUL_KOLOM.len() as u16;
    for (i, isi) in judul_url.iter().enumerate() {
        sheet.write_string_with_format(0, kolom_berkas + i as u16, isi, &judul)?;
    }

    for (i, baris) in semua.iter().enumerate() {
        let r = i as u32 + 1;
        let mut tulis_teks = |kolom: u16, isi: Option<&str>| -> Result<(), XlsxError> {
            if let Some(isi) = isi.filter(|isi| !isi.is_empty()) {
                sheet.write_string_with_format(r, kolom, isi, &teks)?;
            }
            Ok(())
        };
        tulis_teks(1, baris.nama.as_deref())?;
        tulis_teks(2, baris.nim.as_deref())?;
        tulis_teks(3, Some(&baris.email))?;
        tulis_teks(4, baris.jenis_kelamin.as_deref().map(jenis_kelamin))?;
        tulis_teks(5, baris.program_studi.as_deref())?;
        tulis_teks(8, baris.no_hp.as_deref())?;
        tulis_teks(9, Some(&baris.posisi))?;
        tulis_teks(10, Some(&baris.nama_perusahaan))?;
        tulis_teks(11, Some(&label_periode(baris)))?;
        tulis_teks(12, Some(&baris.status))?;
        tulis_teks(17, baris.lokasi_seleksi.as_deref())?;
        tulis_teks(18, baris.nama_dpm.as_deref())?;
        tulis_teks(19, baris.kontak_dpm.as_deref())?;
        tulis_teks(20, baris.nomor_surat.as_deref())?;

        sheet.write_number_with_format(r, 0, f64::from(r), &angka)?;
        if let Some(semester) = baris.semester {
            sheet.write_number_with_format(r, 6, f64::from(semester), &angka)?;
        }
        if let Some(nilai) = baris.ipk {
            sheet.write_number_with_format(r, 7, nilai, &ipk)?;
        }

        let tulis_waktu = |sheet: &mut rust_xlsxwriter::Worksheet,
                           kolom: u16,
                           isi: Option<&str>,
                           format: &Format|
         -> Result<(), XlsxError> {
            if let Some(isi) = isi {
                sheet.write_datetime_with_format(
                    r,
                    kolom,
                    ExcelDateTime::parse_from_str(isi)?,
                    format,
                )?;
            }
            Ok(())
        };
        tulis_waktu(sheet, 13, Some(&baris.created_at), &waktu)?;
        tulis_waktu(sheet, 14, Some(&baris.updated_at), &waktu)?;
        tulis_waktu(sheet, 15, baris.tanggal_seleksi.as_deref(), &tanggal)?;
        tulis_waktu(sheet, 16, baris.jam_seleksi.as_deref(), &jam)?;

        for (item, tautan) in urutan_berkas(baris) {
            let Some(i) = kolom_url.iter().position(|kolom| *kolom == item) else {
                continue;
            };
            sheet.write_url(r, kolom_berkas + i as u16, Url::new(&tautan.url))?;
        }
    }

    let baris_terakhir = semua.len() as u32;
    let kolom_terakhir = kolom_berkas + kolom_url.len() as u16 - 1;
    sheet.set_freeze_panes(1, 0)?;
    sheet.autofilter(0, 0, baris_terakhir, kolom_terakhir)?;
    sheet.autofit();

    let keterangan = workbook.add_worksheet();
    keterangan.set_name("Keterangan")?;
    let tebal = Format::new().set_bold();
    let isi = [
        ("Diekspor", waktu_wib(info.diekspor)),
        ("Jumlah pendaftar", semua.len().to_string()),
        (
            "Link berkas",
            format!(
                "Dapat dibuka siapa pun tanpa login, sampai {}. Setelah itu, minta ekspor ulang ke Panitia Magang.",
                waktu_wib(info.tautan_berlaku_sampai)
            ),
        ),
    ];
    for (r, (label, nilai)) in isi.iter().enumerate() {
        keterangan.write_string_with_format(r as u32, 0, *label, &tebal)?;
        keterangan.write_string(r as u32, 1, nilai)?;
    }
    keterangan.autofit();

    workbook.save_to_buffer()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baris(jenis: &[&str]) -> BarisEkspor {
        BarisEkspor {
            id_pendaftaran: 1,
            nama: None,
            nim: None,
            email: String::new(),
            jenis_kelamin: None,
            program_studi: None,
            semester: None,
            ipk: None,
            no_hp: None,
            posisi: String::new(),
            nama_perusahaan: String::new(),
            catatan_periode: None,
            mulai_periode: String::new(),
            tutup_periode: String::new(),
            status: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            tanggal_seleksi: None,
            jam_seleksi: None,
            lokasi_seleksi: None,
            nama_dpm: None,
            kontak_dpm: None,
            nomor_surat: None,
            berkas: jenis
                .iter()
                .map(|jenis| TautanEkspor {
                    jenis: jenis.to_string(),
                    url: String::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn gives_each_extra_file_a_numbered_column_of_its_own() {
        let semua = [
            baris(&["DPA", "CV", "Tambahan"]),
            baris(&["DPA", "CV"]),
            baris(&["DPA", "CV", "Tambahan", "Tambahan", "Tambahan"]),
        ];

        let (_, judul) = kolom_berkas(&semua);

        assert_eq!(
            judul,
            [
                "URL DPA",
                "URL CV",
                "URL Tambahan 1",
                "URL Tambahan 2",
                "URL Tambahan 3"
            ]
        );
    }

    #[test]
    fn keeps_one_unnumbered_column_for_a_kind_with_one_file_per_row() {
        let semua = [baris(&["DPA", "CV"]), baris(&["DPA", "CV", "Tambahan"])];

        let (_, judul) = kolom_berkas(&semua);

        assert_eq!(judul, ["URL DPA", "URL CV", "URL Tambahan"]);
    }

    #[test]
    fn formats_the_period_in_indonesian() {
        assert_eq!(tanggal_pendek("2026-10-01"), "1 Okt 2026");
        assert_eq!(tanggal_pendek("2026-05-31"), "31 Mei 2026");
        assert_eq!(tanggal_pendek("bukan"), "bukan");
    }
}
