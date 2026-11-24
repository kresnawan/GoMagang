//! Mengekspor rekap penyetaraan mata kuliah (konversi MK) dari pendaftaran pilihan Panitia Magang
//! ke berkas Excel, untuk rekapitulasi nilai akhir oleh program studi di luar portal.

use rust_xlsxwriter::{Color, ExcelDateTime, Format, FormatAlign, Formula, Workbook, XlsxError};
use sqlx::{FromRow, PgPool};
use time::OffsetDateTime;

use super::ekspor::{BarisEkspor, label_periode, waktu_wib};

/// Satu mata kuliah yang dipilih untuk sebuah pendaftaran, beserta data mata kuliahnya saat ini.
#[derive(Debug, FromRow)]
pub struct BarisMk {
    pub id_pendaftaran: i32,
    pub mk_asal: String,
    pub mk_konversi: String,
    pub sks: i32,
    /// Mata kuliah sudah disembunyikan dari katalog setelah dipilih.
    pub dihapus: bool,
    /// YYYY-MM-DD HH:MM:SS, WIB.
    pub created_at: String,
}

/// Mata kuliah yang dipilih untuk pendaftaran-pendaftaran ini, sesuai urutan dipilih. Mata kuliah
/// yang sudah disembunyikan dari katalog tetap ikut, karena sudah terlanjur dipilih.
pub async fn find_mk(db: &PgPool, id_pendaftaran: &[i32]) -> Result<Vec<BarisMk>, sqlx::Error> {
    sqlx::query_as(
        "SELECT ps.id_pendaftaran, mk.mk_asal, mk.mk_konversi, mk.sks,
                mk.deleted_at IS NOT NULL AS dihapus,
                to_char(ps.created_at, 'YYYY-MM-DD HH24:MI:SS') AS created_at
         FROM penyetaraan_mk ps
         JOIN mata_kuliah_konversi mk ON mk.id_mk_konversi = ps.id_mk_konversi
         WHERE ps.id_pendaftaran = ANY($1)
         ORDER BY ps.created_at, ps.id_penyetaraan",
    )
    .bind(id_pendaftaran)
    .fetch_all(db)
    .await
}

/// Kolom identitas pendaftaran, sama di lembar Penyetaraan dan Ringkasan. Kolom `ID Pendaftaran`
/// (kolom B) menjadi kunci rumus di lembar Ringkasan.
const KOLOM_PENDAFTARAN: [&str; 9] = [
    "No",
    "ID Pendaftaran",
    "NIM",
    "Nama",
    "Program Studi",
    "Periode",
    "Perusahaan",
    "Posisi",
    "Status",
];

const KOLOM_MK: [&str; 5] = [
    "MK Asal",
    "MK Konversi",
    "SKS",
    "Dipilih Pada",
    "Keterangan",
];

const KOLOM_RINGKASAN: [&str; 3] = ["Jumlah MK", "Total SKS", "Daftar MK Konversi"];

/// Kolom SKS di lembar Penyetaraan (L), dipakai rumus Total SKS di lembar Ringkasan.
const KOLOM_SKS: u16 = 11;
const HURUF_KOLOM_SKS: &str = "L";

const KETERANGAN_DIHAPUS: &str = "MK sudah dihapus dari katalog";

/// Format sel yang dipakai di semua lembar.
struct Gaya {
    judul: Format,
    teks: Format,
    angka: Format,
    waktu: Format,
    tebal: Format,
}

impl Gaya {
    fn new() -> Self {
        Self {
            judul: Format::new()
                .set_bold()
                .set_font_color(Color::White)
                .set_background_color(Color::RGB(0x202F66))
                .set_align(FormatAlign::VerticalCenter),
            teks: Format::new(),
            angka: Format::new().set_align(FormatAlign::Left),
            waktu: Format::new()
                .set_num_format("dd/mm/yyyy hh:mm")
                .set_align(FormatAlign::Left),
            tebal: Format::new().set_bold(),
        }
    }
}

fn tulis_judul(
    sheet: &mut rust_xlsxwriter::Worksheet,
    judul: impl IntoIterator<Item = &'static str>,
    gaya: &Gaya,
) -> Result<u16, XlsxError> {
    let mut jumlah = 0;
    for (kolom, isi) in judul.into_iter().enumerate() {
        sheet.write_string_with_format(0, kolom as u16, isi, &gaya.judul)?;
        jumlah += 1;
    }
    Ok(jumlah)
}

/// Menulis kolom A-I (identitas pendaftaran) untuk satu baris. Teks selalu ditulis sebagai teks,
/// sehingga nilai dari mahasiswa seperti `=1+1` tidak pernah dijalankan sebagai rumus.
fn tulis_pendaftaran(
    sheet: &mut rust_xlsxwriter::Worksheet,
    r: u32,
    no: usize,
    pendaftaran: &BarisEkspor,
    gaya: &Gaya,
) -> Result<(), XlsxError> {
    sheet.write_number_with_format(r, 0, no as f64, &gaya.angka)?;
    sheet.write_number_with_format(r, 1, f64::from(pendaftaran.id_pendaftaran), &gaya.angka)?;
    let periode = label_periode(pendaftaran);
    let teks = [
        (2, pendaftaran.nim.as_deref()),
        (3, pendaftaran.nama.as_deref()),
        (4, pendaftaran.program_studi.as_deref()),
        (5, Some(periode.as_str())),
        (6, Some(pendaftaran.nama_perusahaan.as_str())),
        (7, Some(pendaftaran.posisi.as_str())),
        (8, Some(pendaftaran.status.as_str())),
    ];
    for (kolom, isi) in teks {
        if let Some(isi) = isi.filter(|isi| !isi.is_empty()) {
            sheet.write_string_with_format(r, kolom, isi, &gaya.teks)?;
        }
    }
    Ok(())
}

/// Menyusun workbook rekap penyetaraan mata kuliah.
///
/// - **Penyetaraan**: satu baris per mata kuliah yang dipilih, urut pendaftaran seperti di
///   `pendaftaran` lalu urut waktu dipilih. Mata kuliah yang sudah dihapus dari katalog diberi
///   keterangan.
/// - **Ringkasan**: satu baris per pendaftaran, termasuk yang belum memilih mata kuliah. Jumlah MK
///   dan Total SKS ditulis sebagai rumus `COUNTIFS`/`SUMIFS` ke lembar Penyetaraan (dengan hasil
///   tersimpan), sehingga ikut berubah bila baris di lembar Penyetaraan disunting.
/// - **Keterangan**: waktu ekspor dan jumlah-jumlah.
///
/// Rumus hanya berisi referensi sel, tidak pernah teks dari pengguna.
pub fn buat_xlsx(
    pendaftaran: &[BarisEkspor],
    mk: &[BarisMk],
    diekspor: OffsetDateTime,
) -> Result<Vec<u8>, XlsxError> {
    let gaya = Gaya::new();
    let mut workbook = Workbook::new();

    // ---- Penyetaraan ----
    let sheet = workbook.add_worksheet();
    sheet.set_name("Penyetaraan")?;
    let jumlah_kolom = tulis_judul(sheet, KOLOM_PENDAFTARAN.into_iter().chain(KOLOM_MK), &gaya)?;
    let mut r = 0;
    for item in pendaftaran {
        for pilihan in mk
            .iter()
            .filter(|pilihan| pilihan.id_pendaftaran == item.id_pendaftaran)
        {
            r += 1;
            tulis_pendaftaran(sheet, r, r as usize, item, &gaya)?;
            sheet.write_string_with_format(r, 9, &pilihan.mk_asal, &gaya.teks)?;
            sheet.write_string_with_format(r, 10, &pilihan.mk_konversi, &gaya.teks)?;
            sheet.write_number_with_format(r, KOLOM_SKS, f64::from(pilihan.sks), &gaya.angka)?;
            sheet.write_datetime_with_format(
                r,
                12,
                ExcelDateTime::parse_from_str(&pilihan.created_at)?,
                &gaya.waktu,
            )?;
            if pilihan.dihapus {
                sheet.write_string_with_format(r, 13, KETERANGAN_DIHAPUS, &gaya.teks)?;
            }
        }
    }
    sheet.set_freeze_panes(1, 0)?;
    sheet.autofilter(0, 0, r, jumlah_kolom - 1)?;
    sheet.autofit();

    // ---- Ringkasan ----
    let sheet = workbook.add_worksheet();
    sheet.set_name("Ringkasan")?;
    let jumlah_kolom = tulis_judul(
        sheet,
        KOLOM_PENDAFTARAN.into_iter().chain(KOLOM_RINGKASAN),
        &gaya,
    )?;
    let mut tanpa_mk = 0;
    for (i, item) in pendaftaran.iter().enumerate() {
        let r = i as u32 + 1;
        let milik: Vec<&BarisMk> = mk
            .iter()
            .filter(|pilihan| pilihan.id_pendaftaran == item.id_pendaftaran)
            .collect();
        if milik.is_empty() {
            tanpa_mk += 1;
        }
        let total_sks: i32 = milik.iter().map(|pilihan| pilihan.sks).sum();

        tulis_pendaftaran(sheet, r, i + 1, item, &gaya)?;
        let baris_excel = r + 1;
        sheet.write_formula_with_format(
            r,
            9,
            Formula::new(format!("=COUNTIFS(Penyetaraan!$B:$B,$B{baris_excel})"))
                .set_result(milik.len().to_string()),
            &gaya.angka,
        )?;
        sheet.write_formula_with_format(
            r,
            10,
            Formula::new(format!(
                "=SUMIFS(Penyetaraan!${HURUF_KOLOM_SKS}:${HURUF_KOLOM_SKS},Penyetaraan!$B:$B,$B{baris_excel})"
            ))
            .set_result(total_sks.to_string()),
            &gaya.angka,
        )?;
        let daftar = milik
            .iter()
            .map(|pilihan| format!("{} ({} SKS)", pilihan.mk_konversi, pilihan.sks))
            .collect::<Vec<_>>()
            .join("; ");
        if !daftar.is_empty() {
            sheet.write_string_with_format(r, 11, &daftar, &gaya.teks)?;
        }
    }
    sheet.set_freeze_panes(1, 0)?;
    sheet.autofilter(0, 0, pendaftaran.len() as u32, jumlah_kolom - 1)?;
    sheet.autofit();

    // ---- Keterangan ----
    let sheet = workbook.add_worksheet();
    sheet.set_name("Keterangan")?;
    let total_sks: i32 = mk.iter().map(|pilihan| pilihan.sks).sum();
    let dihapus = mk.iter().filter(|pilihan| pilihan.dihapus).count();
    let isi = [
        ("Diekspor", waktu_wib(diekspor)),
        ("Jumlah pendaftaran", pendaftaran.len().to_string()),
        ("Belum memilih mata kuliah", tanpa_mk.to_string()),
        ("Jumlah mata kuliah dipilih", mk.len().to_string()),
        ("Total SKS", total_sks.to_string()),
        (
            "Lembar Ringkasan",
            "Jumlah MK dan Total SKS dihitung dari lembar Penyetaraan berdasarkan ID Pendaftaran, \
             jadi ikut berubah bila baris di lembar Penyetaraan diubah atau dihapus."
                .to_string(),
        ),
        (
            "MK dihapus dari katalog",
            format!(
                "{dihapus} pilihan. Mata kuliah ini sudah tidak tersedia di katalog, tetapi sudah dipilih mahasiswa sebelum dihapus."
            ),
        ),
    ];
    for (r, (label, nilai)) in isi.iter().enumerate() {
        sheet.write_string_with_format(r as u32, 0, *label, &gaya.tebal)?;
        sheet.write_string(r as u32, 1, nilai)?;
    }
    sheet.autofit();

    workbook.save_to_buffer()
}
