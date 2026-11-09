use serde::{Deserialize, Serialize};

pub const JUMLAH_SEED: usize = 50;
/// How many extra files a made-up mahasiswa attaches, spread evenly over the mahasiswa.
pub const PILIHAN_JUMLAH_TAMBAHAN: [usize; 4] = [0, 1, 2, 3];

const NAMA_DEPAN: [&str; 30] = [
    "Adi", "Agus", "Aisyah", "Andi", "Anisa", "Bagus", "Bayu", "Budi", "Citra", "Dewi", "Dimas",
    "Eka", "Fajar", "Fitri", "Galih", "Hana", "Indah", "Joko", "Kartika", "Lestari", "Maya",
    "Nanda", "Putri", "Rangga", "Rina", "Sari", "Tari", "Wahyu", "Yoga", "Zahra",
];
const NAMA_BELAKANG: [&str; 30] = [
    "Anggraini",
    "Ardiansyah",
    "Astuti",
    "Firmansyah",
    "Gunawan",
    "Hakim",
    "Halim",
    "Handayani",
    "Hartono",
    "Hidayat",
    "Irawan",
    "Kurniawan",
    "Kusuma",
    "Lubis",
    "Maharani",
    "Mahendra",
    "Nugroho",
    "Pangestu",
    "Permana",
    "Pratama",
    "Purnama",
    "Rahmawati",
    "Saputra",
    "Setiawan",
    "Siregar",
    "Susanto",
    "Utami",
    "Wibowo",
    "Wijaya",
    "Yulianti",
];
const PROGRAM_STUDI: [&str; 5] = [
    "Sistem Informasi",
    "Teknologi Informasi",
    "Pendidikan Teknologi Informasi",
    "Teknik Komputer",
    "Teknik Informatika",
];

fn pilih<'a>(pilihan: &[&'a str]) -> &'a str {
    pilihan[rand::random_range(0..pilihan.len())]
}

/// Refuses every query parameter. An old `?email=` call meant for one mahasiswa must not end up
/// wiping all of them.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TanpaQuery {}

/// A made-up mahasiswa whose data diri is complete and passes the rules of PATCH /mahasiswa/data.
#[derive(Debug)]
pub struct MahasiswaAcak {
    pub email: String,
    pub nim: String,
    pub nama: String,
    pub program_studi: &'static str,
    pub ipk: f64,
    pub no_hp: String,
    pub jenis_kelamin: &'static str,
    pub semester: i16,
}

impl MahasiswaAcak {
    pub fn new() -> Self {
        let depan = pilih(&NAMA_DEPAN);
        let belakang = pilih(&NAMA_BELAKANG);
        let angkatan = rand::random_range(21..=24);
        let nim = format!(
            "{angkatan}515{:010}",
            rand::random_range(0..10_000_000_000u64)
        );

        Self {
            // The NIM keeps the email unique, as many mahasiswa can get the same name.
            email: format!(
                "{}.{}.{nim}@student.ub.ac.id",
                depan.to_lowercase(),
                belakang.to_lowercase()
            ),
            nama: format!("{depan} {belakang}"),
            nim,
            program_studi: pilih(&PROGRAM_STUDI),
            ipk: f64::from(rand::random_range(250..=400)) / 100.0,
            no_hp: format!("08{:010}", rand::random_range(0..10_000_000_000u64)),
            jenis_kelamin: pilih(&["L", "P"]),
            semester: rand::random_range(5..=8),
        }
    }
}

/// What the database no longer has. The files named here are still on disk.
#[derive(Debug)]
pub struct Terhapus {
    pub jumlah_mahasiswa: usize,
    pub jumlah_pendaftaran: usize,
    pub berkas: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct HasilHapus {
    pub jumlah_mahasiswa: usize,
    pub jumlah_pendaftaran: usize,
    pub jumlah_berkas: usize,
    pub jumlah_berkas_gagal: usize,
}

#[derive(Debug, Serialize)]
pub struct HasilSeed {
    pub jumlah_mahasiswa: usize,
    pub jumlah_pendaftaran: usize,
    pub jumlah_berkas: usize,
}

/// A file whose row is saved, and that still has to be written to the upload folder.
#[derive(Debug)]
pub struct BerkasSeed {
    pub nama: String,
    pub isi: Vec<u8>,
}
