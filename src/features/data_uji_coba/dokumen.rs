//! Mockup PDFs for the made-up mahasiswa: real files a browser can open, filled with made-up text.

use rand::seq::SliceRandom;
use time::{OffsetDateTime, macros::offset};

use super::model::MahasiswaAcak;

const BULAN: [&str; 12] = [
    "Januari",
    "Februari",
    "Maret",
    "April",
    "Mei",
    "Juni",
    "Juli",
    "Agustus",
    "September",
    "Oktober",
    "November",
    "Desember",
];
const KEAHLIAN: [&str; 8] = [
    "Pemrograman Python",
    "Pengembangan Web (React, Node.js)",
    "Basis Data SQL",
    "Desain UI/UX dengan Figma",
    "Analisis Data",
    "Jaringan Komputer",
    "Pemrograman Java",
    "Manajemen Proyek",
];
const ORGANISASI: [&str; 6] = [
    "Staf Divisi Acara, Badan Eksekutif Mahasiswa",
    "Anggota, Himpunan Mahasiswa Program Studi",
    "Ketua Panitia, Seminar Nasional Teknologi",
    "Asisten Praktikum, Laboratorium Komputasi",
    "Relawan, Kegiatan Pengabdian Masyarakat",
    "Anggota, Unit Kegiatan Mahasiswa Robotika",
];
const DOSEN: [&str; 6] = [
    "Dr. Ahmad Fauzi, S.Kom., M.Kom.",
    "Dr. Ratna Dewi, S.T., M.T.",
    "Ir. Bambang Wijaya, M.Sc.",
    "Dr. Siti Rahmawati, S.Kom., M.Cs.",
    "Hendra Kurniawan, S.T., M.Eng.",
    "Dr. Lina Marlina, S.Si., M.Kom.",
];
/// The titles an extra file can have, as a mahasiswa may attach any of these.
const JUDUL_TAMBAHAN: [&str; 5] = [
    "PORTFOLIO PROYEK",
    "SERTIFIKAT KOMPETENSI",
    "TRANSKRIP NILAI",
    "SURAT REKOMENDASI",
    "SERTIFIKAT ORGANISASI",
];

fn pilih<'a>(pilihan: &[&'a str]) -> &'a str {
    pilihan[rand::random_range(0..pilihan.len())]
}

/// `jumlah` different items, in random order.
fn pilih_beberapa<'a>(pilihan: &[&'a str], jumlah: usize) -> Vec<&'a str> {
    let mut acak = pilihan.to_vec();
    acak.shuffle(&mut rand::rng());
    acak.truncate(jumlah);
    acak
}

fn tanggal_hari_ini() -> String {
    let hari_ini = OffsetDateTime::now_utc().to_offset(offset!(+7)).date();
    format!(
        "{} {} {}",
        hari_ini.day(),
        BULAN[usize::from(u8::from(hari_ini.month())) - 1],
        hari_ini.year()
    )
}

fn angkatan(mahasiswa: &MahasiswaAcak) -> String {
    format!("20{}", &mahasiswa.nim[..2])
}

pub fn cv(mahasiswa: &MahasiswaAcak) -> Vec<u8> {
    let mut halaman = Halaman::new("CURRICULUM VITAE");
    halaman.bagian("Data Diri");
    halaman.isian("Nama", &mahasiswa.nama);
    halaman.isian("NIM", &mahasiswa.nim);
    halaman.isian("Program Studi", mahasiswa.program_studi);
    halaman.isian("Semester", &mahasiswa.semester.to_string());
    halaman.isian("IPK", &format!("{:.2}", mahasiswa.ipk));
    halaman.isian("Email", &mahasiswa.email);
    halaman.isian("Nomor HP", &mahasiswa.no_hp);

    halaman.bagian("Pendidikan");
    halaman.teks(&format!(
        "Universitas Brawijaya, {} ({} - sekarang)",
        mahasiswa.program_studi,
        angkatan(mahasiswa)
    ));

    halaman.bagian("Keahlian");
    for keahlian in pilih_beberapa(&KEAHLIAN, 3) {
        halaman.teks(&format!("- {keahlian}"));
    }

    halaman.bagian("Pengalaman Organisasi");
    for organisasi in pilih_beberapa(&ORGANISASI, 2) {
        halaman.teks(&format!("- {organisasi}"));
    }

    halaman.selesai()
}

pub fn surat_dpa(mahasiswa: &MahasiswaAcak) -> Vec<u8> {
    let dosen = pilih(&DOSEN);
    let nip = format!("19{:016}", rand::random_range(0..10_000_000_000_000_000u64));

    let mut halaman = Halaman::new("SURAT PERNYATAAN DOSEN PEMBIMBING AKADEMIK");
    halaman.teks("Yang bertanda tangan di bawah ini:");
    halaman.isian("Nama", dosen);
    halaman.isian("NIP", &nip);
    halaman.isian("Jabatan", "Dosen Pembimbing Akademik");
    halaman.jarak();
    halaman.teks("menyatakan bahwa mahasiswa berikut:");
    halaman.isian("Nama", &mahasiswa.nama);
    halaman.isian("NIM", &mahasiswa.nim);
    halaman.isian("Program Studi", mahasiswa.program_studi);
    halaman.jarak();
    halaman.teks("diizinkan untuk mengikuti program Magang Berdampak, dan kegiatan magang");
    halaman.teks("tersebut dapat dikonversikan ke mata kuliah sesuai ketentuan yang berlaku.");
    halaman.jarak();
    halaman.teks("Demikian surat pernyataan ini dibuat untuk dipergunakan sebagaimana mestinya.");
    halaman.jarak();
    halaman.teks(&format!("Malang, {}", tanggal_hari_ini()));
    halaman.teks("Dosen Pembimbing Akademik,");
    halaman.jarak();
    halaman.jarak();
    halaman.teks(dosen);

    halaman.selesai()
}

/// `jumlah` extra files, each with a different title.
pub fn tambahan(mahasiswa: &MahasiswaAcak, jumlah: usize) -> Vec<Vec<u8>> {
    pilih_beberapa(&JUDUL_TAMBAHAN, jumlah)
        .into_iter()
        .map(|judul| {
            let mut halaman = Halaman::new(judul);
            halaman.isian("Milik", &mahasiswa.nama);
            halaman.isian("NIM", &mahasiswa.nim);
            halaman.isian("Program Studi", mahasiswa.program_studi);
            halaman.bagian("Rincian");
            for keahlian in pilih_beberapa(&KEAHLIAN, 3) {
                halaman.teks(&format!("- {keahlian}"));
            }
            halaman.teks(&format!("Diterbitkan di Malang, {}", tanggal_hari_ini()));
            halaman.selesai()
        })
        .collect()
}

const LEBAR: f32 = 595.0;
const TINGGI: f32 = 842.0;
const KIRI: f32 = 56.0;
/// How far right of the labels the values start.
const KOLOM_NILAI: f32 = 100.0;

/// One A4 page of text, written top to bottom.
struct Halaman {
    isi: String,
    y: f32,
}

impl Halaman {
    fn new(judul: &str) -> Self {
        let mut halaman = Self {
            isi: String::new(),
            y: TINGGI - 72.0,
        };
        halaman.tulis(judul, "F2", 15.0);
        // A line under the title.
        halaman.y -= 8.0;
        halaman.isi.push_str(&format!(
            "0.6 w {KIRI} {y} m {kanan} {y} l S\n",
            y = halaman.y,
            kanan = LEBAR - KIRI
        ));
        halaman.y -= 22.0;
        halaman
    }

    fn bagian(&mut self, judul: &str) {
        self.y -= 10.0;
        self.tulis(judul, "F2", 12.0);
        self.y -= 4.0;
    }

    fn teks(&mut self, teks: &str) {
        self.tulis(teks, "F1", 11.0);
    }

    /// A label and its value on one line, with the values lined up under each other.
    fn isian(&mut self, label: &str, nilai: &str) {
        self.isi.push_str(&format!(
            "BT /F1 11 Tf {KIRI} {y} Td ({label}) Tj ET\n\
             BT /F1 11 Tf {x} {y} Td (: {nilai}) Tj ET\n",
            y = self.y,
            x = KIRI + KOLOM_NILAI,
            label = escape(label),
            nilai = escape(nilai)
        ));
        self.y -= 17.0;
    }

    fn jarak(&mut self) {
        self.y -= 12.0;
    }

    fn tulis(&mut self, teks: &str, font: &str, ukuran: f32) {
        self.isi.push_str(&format!(
            "BT /{font} {ukuran} Tf {KIRI} {y} Td ({teks}) Tj ET\n",
            y = self.y,
            teks = escape(teks)
        ));
        self.y -= ukuran + 6.0;
    }

    /// The page, with a note at the bottom that it is not a real document, as a complete PDF file.
    fn selesai(mut self) -> Vec<u8> {
        self.isi.push_str(&format!(
            "0.5 g BT /F1 9 Tf {KIRI} 40 Td ({}) Tj ET\n",
            escape("Dokumen contoh untuk uji coba GoMagang. Bukan dokumen asli.")
        ));
        pdf(&self.isi)
    }
}

/// Escapes the characters that end or break a PDF string. Anything outside ASCII becomes `?`, as
/// the built-in fonts are used without an encoding for it.
fn escape(teks: &str) -> String {
    let mut hasil = String::with_capacity(teks.len());
    for karakter in teks.chars() {
        match karakter {
            '\\' | '(' | ')' => {
                hasil.push('\\');
                hasil.push(karakter);
            }
            ' '..='~' => hasil.push(karakter),
            _ => hasil.push('?'),
        }
    }
    hasil
}

/// A single-page PDF with Helvetica and Helvetica-Bold, drawing this content stream.
fn pdf(konten: &str) -> Vec<u8> {
    let objek = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {LEBAR} {TINGGI}] \
             /Resources << /Font << /F1 4 0 R /F2 5 0 R >> >> /Contents 6 0 R >>"
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>".to_string(),
        format!(
            "<< /Length {} >>\nstream\n{konten}\nendstream",
            konten.len()
        ),
    ];

    let mut hasil = b"%PDF-1.4\n".to_vec();
    let mut posisi = Vec::with_capacity(objek.len());
    for (i, isi) in objek.iter().enumerate() {
        posisi.push(hasil.len());
        hasil.extend_from_slice(format!("{} 0 obj\n{isi}\nendobj\n", i + 1).as_bytes());
    }

    // Each cross-reference entry is exactly 20 bytes, as readers expect.
    let xref = hasil.len();
    let mut tabel = format!("xref\n0 {}\n0000000000 65535 f \n", objek.len() + 1);
    for posisi in posisi {
        tabel.push_str(&format!("{posisi:010} 00000 n \n"));
    }
    tabel.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objek.len() + 1
    ));
    hasil.extend_from_slice(tabel.as_bytes());
    hasil
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mahasiswa() -> MahasiswaAcak {
        let mut mahasiswa = MahasiswaAcak::new();
        mahasiswa.nama = "Budi (Santoso) \\ Andreé".to_string();
        mahasiswa
    }

    /// Checks the structure a PDF reader relies on: the header, every object where the
    /// cross-reference table says it is, and the end marker.
    fn assert_valid_pdf(isi: &[u8]) {
        let teks = String::from_utf8(isi.to_vec()).unwrap();
        assert!(teks.starts_with("%PDF-1.4\n"));
        assert!(teks.ends_with("%%EOF\n"));

        let startxref: usize = teks
            .rsplit("startxref\n")
            .next()
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(teks[startxref..].starts_with("xref\n0 7\n"));

        let entries: Vec<&str> = teks[startxref..].lines().skip(3).take(6).collect();
        for (i, entry) in entries.iter().enumerate() {
            assert_eq!(entry.len() + 1, 20, "{entry:?}");
            let posisi: usize = entry[..10].parse().unwrap();
            assert!(
                teks[posisi..].starts_with(&format!("{} 0 obj\n", i + 1)),
                "object {}",
                i + 1
            );
        }

        let (_, stream) = teks.split_once(">>\nstream\n").unwrap();
        let panjang: usize = teks
            .split("/Length ")
            .nth(1)
            .unwrap()
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(
            &stream[panjang..panjang + "\nendstream".len()],
            "\nendstream"
        );
    }

    #[test]
    fn every_document_is_a_well_formed_pdf() {
        let mahasiswa = mahasiswa();
        assert_valid_pdf(&cv(&mahasiswa));
        assert_valid_pdf(&surat_dpa(&mahasiswa));
        for isi in tambahan(&mahasiswa, 3) {
            assert_valid_pdf(&isi);
        }
    }

    #[test]
    fn the_documents_carry_the_mahasiswa_data() {
        let mahasiswa = mahasiswa();
        for isi in [cv(&mahasiswa), surat_dpa(&mahasiswa)] {
            let teks = String::from_utf8(isi).unwrap();
            assert!(teks.contains(&mahasiswa.nim));
            assert!(teks.contains(r"Budi \(Santoso\) \\ Andre?"));
            assert!(teks.contains("Bukan dokumen asli."));
        }
    }

    #[test]
    fn extra_files_have_different_titles() {
        let mahasiswa = mahasiswa();
        for jumlah in 0..=3 {
            let berkas = tambahan(&mahasiswa, jumlah);
            assert_eq!(berkas.len(), jumlah);
            let mut judul: Vec<&str> = JUDUL_TAMBAHAN
                .iter()
                .copied()
                .filter(|judul| {
                    berkas
                        .iter()
                        .any(|isi| String::from_utf8_lossy(isi).contains(&format!("({judul})")))
                })
                .collect();
            judul.dedup();
            assert_eq!(judul.len(), jumlah);
        }
    }
}
