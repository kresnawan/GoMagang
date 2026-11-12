//! Parts of the surat that have no data source yet. Everything marked MOCK is a stand-in and
//! should be replaced once the real data exists (see DOCUMENTATION.md).

use time::{Date, Duration};

pub const KOTA: &str = "Malang";
pub const JABATAN_PEJABAT: &str = "Wakil Dekan Bidang Akademik";

/// MOCK: the official and their NIP are not stored anywhere yet.
pub const NAMA_PEJABAT: &str = "[Nama Pejabat]";
pub const NIP_PEJABAT: &str = "[NIP]";

/// MOCK: the kind of programme is not stored yet.
pub const PROGRAM: &str = "Magang Mandiri";

/// MOCK: perusahaan_mitra only has a name.
pub const ALAMAT_PERUSAHAAN: &str = "Jl. Contoh Alamat Instansi No. 1, Kota Malang";

/// MOCK: only the registration period is stored, not when the internship itself runs. Until it
/// is, the internship is taken to start on the day the surat is issued and to last this long.
const LAMA_MAGANG: Duration = Duration::days(120);

/// MOCK: menghitung tanggal mulai dan selesai magang yang dicetak di surat.
///
/// Saat ini yang tersimpan hanya periode pendaftaran, bukan waktu pelaksanaan magang. Sampai data
/// itu ada, magang dianggap mulai pada tanggal surat terbit dan berlangsung selama
/// [`LAMA_MAGANG`] (120 hari).
pub fn masa_magang(tanggal_surat: Date) -> (Date, Date) {
    (tanggal_surat, tanggal_surat + LAMA_MAGANG)
}

/// MOCK: menyusun nomor surat, misalnya `00007/SKM/MB-FILKOM/2026`.
///
/// Formatnya: id surat lima digit, kode tetap, lalu tahun tanggal surat. Skema penomoran resmi
/// fakultas belum diketahui, sehingga format ini perlu diganti setelah ada.
pub fn nomor_surat(id_surat: i32, tanggal_surat: Date) -> String {
    format!("{id_surat:05}/SKM/MB-FILKOM/{}", tanggal_surat.year())
}

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

/// Menulis tanggal dengan nama bulan bahasa Indonesia, misalnya `6 November 2026`.
pub fn tanggal_panjang(tanggal: Date) -> String {
    let bulan = BULAN[usize::from(u8::from(tanggal.month())) - 1];
    format!("{} {bulan} {}", tanggal.day(), tanggal.year())
}

#[cfg(test)]
mod tests {
    use time::macros::date;

    use super::*;

    #[test]
    fn formats_dates_in_indonesian() {
        assert_eq!(tanggal_panjang(date!(2026 - 11 - 06)), "6 November 2026");
        assert_eq!(tanggal_panjang(date!(2027 - 01 - 31)), "31 Januari 2027");
    }

    #[test]
    fn numbers_surat_by_id_and_year() {
        assert_eq!(
            nomor_surat(7, date!(2026 - 11 - 06)),
            "00007/SKM/MB-FILKOM/2026"
        );
    }
}
