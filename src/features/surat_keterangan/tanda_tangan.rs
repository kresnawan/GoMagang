use aws_lc_rs::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};

/// Awalan untuk semua yang ditandatangani, sehingga tanda tangan dari sini tidak bisa diaku sebagai
/// tanda tangan atas jenis data lain yang ditandatangani dengan kunci yang sama.
const KONTEKS: &[u8] = b"gomagang/surat-keterangan/v1\n";

/// Menandatangani dan memeriksa surat keterangan dengan kunci Ed25519 dari `SURAT_SIGNING_KEY`.
/// Hanya server yang memegang kunci ini, sehingga tidak ada pihak lain yang bisa membuat tanda
/// tangan yang lolos `cocok`.
pub struct PenandaSurat {
    kunci: Ed25519KeyPair,
}

impl PenandaSurat {
    /// Membuat penanda tangan dari seed Ed25519 32 byte dalam base64 standar (nilai
    /// `SURAT_SIGNING_KEY`).
    ///
    /// # Panics
    /// Panic bila seed bukan base64, panjangnya bukan 32 byte, atau tidak bisa dipakai sebagai
    /// kunci
    /// Ed25519. Sama seperti secret lain, kesalahan ini sengaja dibuat ketahuan saat server start.
    pub fn from_base64_seed(seed: &str) -> Self {
        let seed = STANDARD
            .decode(seed.trim())
            .expect("SURAT_SIGNING_KEY must be base64");
        assert_eq!(seed.len(), 32, "SURAT_SIGNING_KEY must decode to 32 bytes");
        let kunci = Ed25519KeyPair::from_seed_unchecked(&seed)
            .expect("SURAT_SIGNING_KEY is not a valid Ed25519 seed");
        Self { kunci }
    }

    /// Menandatangani `isi` dan mengembalikan tanda tangan dalam base64url tanpa padding.
    ///
    /// Yang ditandatangani adalah `isi` yang diawali konteks `KONTEKS`, sehingga
    /// tanda tangan ini tidak bisa disalahgunakan sebagai tanda tangan atas jenis data lain. Format
    /// base64url dipilih karena tanda tangan ikut masuk ke URL di QR code.
    pub fn tandatangani(&self, isi: &str) -> String {
        URL_SAFE_NO_PAD.encode(self.kunci.sign(&pesan(isi)).as_ref())
    }

    /// Memeriksa apakah `tanda_tangan` (base64url) sah untuk `isi` dengan kunci server ini.
    ///
    /// Mengembalikan `false` bila tanda tangan bukan base64url yang valid, dibuat dengan kunci
    /// lain, atau
    /// `isi` berubah walau hanya satu byte.
    pub fn cocok(&self, isi: &str, tanda_tangan: &str) -> bool {
        let Ok(tanda_tangan) = URL_SAFE_NO_PAD.decode(tanda_tangan) else {
            return false;
        };
        UnparsedPublicKey::new(&ED25519, self.kunci.public_key().as_ref())
            .verify(&pesan(isi), &tanda_tangan)
            .is_ok()
    }

    /// Kunci publik Ed25519 dalam base64 standar.
    ///
    /// Dibagikan agar siapa pun bisa memeriksa tanda tangan surat secara mandiri, tanpa harus
    /// mempercayai jawaban server ini.
    pub fn kunci_publik(&self) -> String {
        STANDARD.encode(self.kunci.public_key().as_ref())
    }
}

/// Menyusun byte yang benar-benar ditandatangani: [`KONTEKS`] diikuti isi surat.
fn pesan(isi: &str) -> Vec<u8> {
    [KONTEKS, isi.as_bytes()].concat()
}

/// Menyusun kode verifikasi yang dimuat di QR code: `{id_surat}.{tanda_tangan}`.
///
/// Id dipakai untuk menemukan baris surat, sedangkan tanda tangan membuktikan bahwa kode ini memang
/// diterbitkan server, bukan hasil tebakan id.
pub fn kode(id_surat: i32, tanda_tangan: &str) -> String {
    format!("{id_surat}.{tanda_tangan}")
}

/// Memecah kode verifikasi `{id_surat}.{tanda_tangan}` menjadi id dan tanda tangan.
///
/// Spasi di awal dan akhir diabaikan. Mengembalikan `None` bila tidak ada titik, id bukan angka
/// positif, atau bagian tanda tangan kosong.
pub fn parse_kode(kode: &str) -> Option<(i32, &str)> {
    let (id, tanda_tangan) = kode.trim().split_once('.')?;
    let id = id.parse().ok().filter(|id: &i32| *id > 0)?;
    (!tanda_tangan.is_empty()).then_some((id, tanda_tangan))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";

    #[test]
    fn signature_covers_exactly_the_content() {
        let penanda = PenandaSurat::from_base64_seed(SEED);
        let tanda_tangan = penanda.tandatangani(r#"{"nama":"Budi"}"#);

        assert!(penanda.cocok(r#"{"nama":"Budi"}"#, &tanda_tangan));
        assert!(!penanda.cocok(r#"{"nama":"Budi "}"#, &tanda_tangan));
        assert!(!penanda.cocok(r#"{"nama":"Budi"}"#, "bukan-tanda-tangan"));
    }

    #[test]
    fn another_key_does_not_verify() {
        let asli = PenandaSurat::from_base64_seed(SEED);
        let lain = PenandaSurat::from_base64_seed(&STANDARD.encode([7u8; 32]));
        let tanda_tangan = lain.tandatangani("isi");

        assert!(!asli.cocok("isi", &tanda_tangan));
    }

    #[test]
    fn parses_codes() {
        assert_eq!(parse_kode("12.abc"), Some((12, "abc")));
        assert_eq!(parse_kode(" 12.abc "), Some((12, "abc")));
        for salah in ["", "12", "12.", ".abc", "0.abc", "-1.abc", "x.abc"] {
            assert_eq!(parse_kode(salah), None, "{salah}");
        }
    }
}
