//! Menyusun surat sebagai PDF A4 satu halaman, mengikuti templat fakultas. Posisi diukur dari atas
//! halaman dalam satuan point; posisi teks adalah baseline-nya.
//!
//! Tidak ada bagian yang bergantung pada jam atau angka acak, sehingga isi yang sama selalu
//! menghasilkan byte yang sama.

use pdf_writer::{
    Content, Filter, Finish, Name, Pdf, Rect, Ref, Str, TextStr,
    types::{ActionType, AnnotationType},
};
use qrcodegen::{DataTooLong, QrCode, QrCodeEcc};
use time::{Date, macros::format_description};

use super::{huruf::Huruf, model::IsiSurat, templat};

const LEBAR: f32 = 595.28;
const TINGGI: f32 = 841.89;
const KIRI: f32 = 74.0;
const KANAN: f32 = 529.0;

const UKURAN: f32 = 12.0;
const SPASI_BARIS: f32 = 15.2;

const F_BIASA: Name<'static> = Name(b"F1");
const F_TEBAL: Name<'static> = Name(b"F2");

const HITAM: [f32; 3] = [0.0, 0.0, 0.0];
const ABU: [f32; 3] = [0.45, 0.45, 0.45];
const BIRU: [f32; 3] = [0.07, 0.33, 0.8];

/// Posisi kop surat fakultas, di samping logo.
const KOP_X: f32 = 225.0;
const ALAMAT_FAKULTAS: &str =
    "Jl. Veteran, Malang 65145, Jawa Timur, Indonesia, Telp. (0341) 577-911";

/// Ukuran QR code di ruang tanda tangan.
const UKURAN_QR: f32 = 74.0;

struct Kanvas {
    isi: Content,
}

impl Kanvas {
    /// Nama resource font PDF untuk jenis huruf: `F1` untuk biasa (Times-Roman) dan `F2` untuk
    /// tebal
    /// (Times-Bold).
    fn font(huruf: Huruf) -> Name<'static> {
        match huruf {
            Huruf::Biasa => F_BIASA,
            Huruf::Tebal => F_TEBAL,
        }
    }

    /// Menulis teks yang sudah di-encode pada posisi `x` dan baseline `atas` (diukur dari atas
    /// halaman), dengan huruf, ukuran, dan warna tertentu.
    fn teks(&mut self, huruf: Huruf, ukuran: f32, x: f32, atas: f32, teks: &[u8], warna: [f32; 3]) {
        self.isi
            .set_fill_rgb(warna[0], warna[1], warna[2])
            .begin_text()
            .set_font(Self::font(huruf), ukuran)
            .next_line(x, TINGGI - atas)
            .show(Str(teks))
            .end_text();
    }

    /// Menulis satu baris teks hitam yang diratakan ke tengah halaman.
    fn teks_tengah(&mut self, huruf: Huruf, ukuran: f32, atas: f32, teks: &str) {
        let teks = huruf.encode(teks);
        let x = (LEBAR - huruf.lebar(&teks, ukuran)) / 2.0;
        self.teks(huruf, ukuran, x, atas, &teks, HITAM);
    }

    /// Menulis paragraf mulai dari `x`, dibungkus otomatis di margin kanan.
    ///
    /// Mengembalikan baseline baris terakhir, sehingga elemen berikutnya bisa diletakkan di
    /// bawahnya.
    fn paragraf(&mut self, x: f32, atas: f32, teks: &str) -> f32 {
        let mut baseline = atas;
        for (i, baris) in Huruf::Biasa
            .pecah_baris(teks, UKURAN, KANAN - x)
            .iter()
            .enumerate()
        {
            baseline = atas + i as f32 * SPASI_BARIS;
            self.teks(Huruf::Biasa, UKURAN, x, baseline, baris, HITAM);
        }
        baseline
    }

    /// Menggambar garis horizontal dari `x1` ke `x2` pada ketinggian `atas`, dengan ketebalan dan
    /// warna
    /// tertentu.
    fn garis(&mut self, x1: f32, x2: f32, atas: f32, tebal: f32, warna: [f32; 3]) {
        self.isi
            .set_stroke_rgb(warna[0], warna[1], warna[2])
            .set_line_width(tebal)
            .move_to(x1, TINGGI - atas)
            .line_to(x2, TINGGI - atas)
            .stroke();
    }

    /// Menulis satu baris yang terdiri dari potongan teks biasa dan potongan bergaya tautan (biru
    /// dan
    /// bergaris bawah).
    ///
    /// Ukuran font diperkecil bila perlu, maksimal `ukuran_maks`, agar seluruh baris muat di antara
    /// `x`
    /// dan margin kanan. Dipakai untuk catatan verifikasi yang memuat alamat halaman verifikasi.
    fn baris_campur(&mut self, x: f32, atas: f32, ukuran_maks: f32, potongan: &[(&str, bool)]) {
        let potongan: Vec<(Vec<u8>, bool)> = potongan
            .iter()
            .map(|(teks, tautan)| (Huruf::Biasa.encode(teks), *tautan))
            .collect();
        let lebar_satuan: f32 = potongan
            .iter()
            .map(|(teks, _)| Huruf::Biasa.lebar(teks, 1.0))
            .sum();
        let ukuran = ukuran_maks.min((KANAN - x) / lebar_satuan);

        let mut x = x;
        for (teks, tautan) in &potongan {
            let lebar = Huruf::Biasa.lebar(teks, ukuran);
            let warna = if *tautan { BIRU } else { HITAM };
            self.teks(Huruf::Biasa, ukuran, x, atas, teks, warna);
            if *tautan {
                self.garis(x, x + lebar, atas + 1.2, 0.5, BIRU);
            }
            x += lebar;
        }
    }

    /// Menggambar QR code dengan sudut kiri atas di (`x`, `atas`) dan sisi sepanjang `ukuran`
    /// point.
    ///
    /// Modul gelap yang bersebelahan dalam satu baris digabung menjadi satu persegi panjang,
    /// sehingga
    /// jumlah perintah gambar di PDF jauh berkurang.
    fn qr(&mut self, qr: &QrCode, x: f32, atas: f32, ukuran: f32) {
        let modul = ukuran / qr.size() as f32;
        self.isi.set_fill_rgb(0.0, 0.0, 0.0);
        for baris in 0..qr.size() {
            let mut kolom = 0;
            while kolom < qr.size() {
                if !qr.get_module(kolom, baris) {
                    kolom += 1;
                    continue;
                }
                let awal = kolom;
                while kolom < qr.size() && qr.get_module(kolom, baris) {
                    kolom += 1;
                }
                let y = TINGGI - atas - (baris + 1) as f32 * modul;
                self.isi.rect(
                    x + awal as f32 * modul,
                    y,
                    (kolom - awal) as f32 * modul,
                    modul,
                );
            }
        }
        self.isi.fill_nonzero();
    }
}

/// Menulis tanggal ISO `YYYY-MM-DD` seperti di surat, misalnya `6 November 2026`.
///
/// Spasinya diganti spasi tak terputus agar tanggal tidak pernah terpotong ke dua baris. Isi surat
/// selalu dibuat dari `Date` yang valid, sehingga cabang cadangan (teks asli dikembalikan apa
/// adanya) tidak diharapkan terjadi.
fn tanggal(iso: &str) -> String {
    Date::parse(iso, format_description!("[year]-[month]-[day]"))
        .map(templat::tanggal_panjang)
        .unwrap_or_else(|_| iso.to_string())
        .replace(' ', "\u{a0}")
}

/// Mengompres content stream halaman dengan zlib (filter `FlateDecode`).
///
/// QR code saja terdiri dari ratusan persegi panjang, sehingga kompresi memperkecil isi halaman
/// beberapa kali lipat.
fn kompres(isi: &[u8]) -> Vec<u8> {
    use std::io::Write;

    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    encoder
        .write_all(isi)
        .and_then(|()| encoder.finish())
        .expect("writing to memory cannot fail")
}

/// Menyusun surat keterangan sebagai PDF A4 satu halaman mengikuti templat fakultas.
///
/// Isi halaman: kop surat, judul dan nomor, data mahasiswa, isi keterangan magang, penanda tangan
/// dengan QR code di tempat tanda tangan basah, dan catatan cara verifikasi.
///
/// - `url_verifikasi`: tautan lengkap beserta kode yang dibuka QR code. QR code juga bisa diklik di
///   pembaca PDF.
/// - `halaman_verifikasi`: alamat halaman verifikasi tanpa kode, dicetak untuk orang yang mengetik
///   alamatnya sendiri.
///
/// Hasilnya deterministik: tidak bergantung pada jam atau angka acak, sehingga isi yang sama selalu
/// menghasilkan byte yang sama. Gagal hanya bila URL terlalu panjang untuk QR code.
pub fn render(
    isi: &IsiSurat,
    url_verifikasi: &str,
    halaman_verifikasi: &str,
) -> Result<Vec<u8>, DataTooLong> {
    let qr = QrCode::encode_text(url_verifikasi, QrCodeEcc::Medium)?;
    let mut k = Kanvas {
        isi: Content::new(),
    };

    // Kop surat. MOCK: a frame stands in for the faculty logo until the image is supplied.
    k.isi
        .set_stroke_rgb(ABU[0], ABU[1], ABU[2])
        .set_line_width(0.6)
        .rect(79.0, TINGGI - 146.0, 125.0, 42.0)
        .stroke();
    let logo = Huruf::Tebal.encode("LOGO FILKOM");
    let x_logo = 79.0 + (125.0 - Huruf::Tebal.lebar(&logo, 9.0)) / 2.0;
    k.teks(Huruf::Tebal, 9.0, x_logo, 128.0, &logo, ABU);

    k.teks(
        Huruf::Tebal,
        14.0,
        KOP_X,
        110.0,
        &Huruf::Tebal.encode("FAKULTAS ILMU KOMPUTER"),
        HITAM,
    );
    k.teks(
        Huruf::Biasa,
        14.0,
        KOP_X,
        127.0,
        &Huruf::Biasa.encode("UNIVERSITAS BRAWIJAYA"),
        HITAM,
    );
    k.baris_campur(KOP_X, 141.0, 9.5, &[(ALAMAT_FAKULTAS, false)]);
    k.baris_campur(
        KOP_X,
        154.0,
        9.5,
        &[
            ("Fax. (0341) 577-911 ", false),
            ("https://filkom.ub.ac.id/", true),
            (" , e-mail:", false),
            ("filkom@ub.ac.id", true),
        ],
    );
    k.garis(77.0, KANAN, 174.0, 0.8, ABU);

    // Judul
    k.teks_tengah(
        Huruf::Tebal,
        UKURAN,
        205.0,
        "SURAT KETERANGAN KEPESERTAAN MAGANG",
    );
    k.teks_tengah(
        Huruf::Biasa,
        UKURAN,
        221.0,
        &format!("Nomor: {}", isi.nomor_surat),
    );

    // Data mahasiswa
    k.paragraf(
        KIRI,
        252.0,
        "Dengan adanya surat ini maka dinyatakan kepesertaan mahasiswa:",
    );
    let mut atas = 285.0;
    for (label, nilai) in [
        ("Nama", isi.nama.as_str()),
        ("NIM", isi.nim.as_str()),
        ("Program Studi", isi.program_studi.as_str()),
    ] {
        k.teks(
            Huruf::Biasa,
            UKURAN,
            KIRI,
            atas,
            &Huruf::Biasa.encode(label),
            HITAM,
        );
        k.teks(Huruf::Biasa, UKURAN, 164.0, atas, b":", HITAM);
        atas = k.paragraf(170.0, atas, nilai) + 15.5;
    }

    // Isi
    let isi_surat = format!(
        "Bahwa mahasiswa terkait benar sedang melaksanakan kegiatan magang di {}, yang beralamat \
         di {}, pada periode {} sampai dengan {}, sebagai {}. Kegiatan ini merupakan bagian dari \
         program {} yang diakui oleh Fakultas Ilmu Komputer Universitas Brawijaya.",
        isi.perusahaan,
        isi.alamat_perusahaan,
        tanggal(&isi.tanggal_mulai),
        tanggal(&isi.tanggal_selesai),
        isi.posisi,
        isi.program,
    );
    let atas = k.paragraf(KIRI, atas + 17.5, &isi_surat);
    let atas = k.paragraf(
        KIRI,
        atas + 36.0,
        "Demikian surat keterangan ini dibuat untuk dipergunakan sebagaimana mestinya.",
    );

    // Penanda tangan, with the QR in the space a wet signature would take
    let atas = k.paragraf(
        KIRI,
        atas + 34.0,
        &format!("{}, {}", templat::KOTA, tanggal(&isi.tanggal_surat)),
    );
    let atas = k.paragraf(
        KIRI,
        atas + SPASI_BARIS,
        &format!("{},", isi.jabatan_pejabat),
    );
    let atas_qr = atas + 10.0;
    k.qr(&qr, KIRI, atas_qr, UKURAN_QR);
    let atas = k.paragraf(KIRI, atas_qr + UKURAN_QR + 16.0, &isi.nama_pejabat);
    k.paragraf(
        KIRI,
        atas + SPASI_BARIS,
        &format!("NIP. {}", isi.nip_pejabat),
    );

    // Catatan verifikasi
    k.garis(KIRI, KANAN, 772.0, 0.5, ABU);
    let catatan = format!(
        "Surat ini diterbitkan dan ditandatangani secara elektronik melalui portal GoMagang. \
         Keasliannya dapat diperiksa dengan memindai kode QR di atas, atau dengan mengunggah \
         berkas PDF ini di {halaman_verifikasi}."
    );
    for (i, baris) in Huruf::Biasa
        .pecah_baris(&catatan, 8.5, KANAN - KIRI)
        .iter()
        .enumerate()
    {
        k.teks(Huruf::Biasa, 8.5, KIRI, 785.0 + i as f32 * 10.5, baris, ABU);
    }

    let mut pdf = Pdf::new();
    let katalog = Ref::new(1);
    let pohon = Ref::new(2);
    let halaman = Ref::new(3);
    let konten = Ref::new(4);
    let biasa = Ref::new(5);
    let tebal = Ref::new(6);
    let tautan_qr = Ref::new(7);
    let info = Ref::new(8);

    pdf.catalog(katalog).pages(pohon);
    pdf.pages(pohon).kids([halaman]).count(1);

    let mut page = pdf.page(halaman);
    page.media_box(Rect::new(0.0, 0.0, LEBAR, TINGGI));
    page.parent(pohon);
    page.contents(konten);
    page.annotations([tautan_qr]);
    page.resources()
        .fonts()
        .pair(F_BIASA, biasa)
        .pair(F_TEBAL, tebal);
    page.finish();

    // Clicking the QR in a PDF reader opens the same page as scanning it.
    let mut annotation = pdf.annotation(tautan_qr);
    annotation.subtype(AnnotationType::Link);
    annotation.rect(Rect::new(
        KIRI,
        TINGGI - atas_qr - UKURAN_QR,
        KIRI + UKURAN_QR,
        TINGGI - atas_qr,
    ));
    annotation.contents(TextStr("Verifikasi keaslian surat"));
    annotation.border(0.0, 0.0, 0.0, None);
    annotation
        .action()
        .action_type(ActionType::Uri)
        .uri(Str(url_verifikasi.as_bytes()));
    annotation.finish();

    for (id, nama) in [
        (biasa, b"Times-Roman".as_slice()),
        (tebal, b"Times-Bold".as_slice()),
    ] {
        pdf.type1_font(id)
            .base_font(Name(nama))
            .encoding_predefined(Name(b"WinAnsiEncoding"));
    }

    pdf.document_info(info)
        .title(TextStr(&format!(
            "Surat Keterangan Kepesertaan Magang {}",
            isi.nomor_surat
        )))
        .producer(TextStr("GoMagang"));

    pdf.stream(konten, &kompres(&k.isi.finish()))
        .filter(Filter::FlateDecode);
    Ok(pdf.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::surat_keterangan::model::VERSI_ISI;

    pub fn contoh() -> IsiSurat {
        IsiSurat {
            versi: VERSI_ISI,
            nomor_surat: "00012/SKM/MB-FILKOM/2026".to_string(),
            tanggal_surat: "2026-11-06".to_string(),
            nama: "Budi Santoso".to_string(),
            nim: "215150200111001".to_string(),
            program_studi: "S1 Sistem Informasi".to_string(),
            posisi: "Intern Business Analyst".to_string(),
            perusahaan: "PT Nusantara Digital".to_string(),
            alamat_perusahaan: templat::ALAMAT_PERUSAHAAN.to_string(),
            program: templat::PROGRAM.to_string(),
            tanggal_mulai: "2026-11-06".to_string(),
            tanggal_selesai: "2027-03-06".to_string(),
            nama_pejabat: templat::NAMA_PEJABAT.to_string(),
            nip_pejabat: templat::NIP_PEJABAT.to_string(),
            jabatan_pejabat: templat::JABATAN_PEJABAT.to_string(),
        }
    }

    const HALAMAN: &str = "http://localhost:5173/verifikasi-surat";
    const URL: &str = "http://localhost:5173/verifikasi-surat?kode=12.\
        Zm9vYmFyZm9vYmFyZm9vYmFyZm9vYmFyZm9vYmFyZm9vYmFyZm9vYmFyZm9vYmFyZm9vYmFyZm9vYmFy";

    #[test]
    fn same_content_gives_the_same_bytes() {
        let pdf = render(&contoh(), URL, HALAMAN).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert_eq!(pdf, render(&contoh(), URL, HALAMAN).unwrap());
    }

    #[test]
    fn different_content_gives_different_bytes() {
        let mut lain = contoh();
        lain.nama = "Budi Santosa".to_string();
        assert_ne!(
            render(&contoh(), URL, HALAMAN).unwrap(),
            render(&lain, URL, HALAMAN).unwrap()
        );
    }

    /// Menulis contoh surat untuk dilihat saat menyesuaikan tata letak:
    /// `SURAT_PREVIEW=/tmp/surat.pdf cargo test surat_preview -- --ignored`
    #[test]
    #[ignore]
    fn surat_preview() {
        let path = std::env::var("SURAT_PREVIEW").expect("set SURAT_PREVIEW to a .pdf path");
        std::fs::write(path, render(&contoh(), URL, HALAMAN).unwrap()).unwrap();
    }
}
