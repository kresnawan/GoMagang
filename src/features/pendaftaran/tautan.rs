//! Public links to berkas pendaftaran, for the people outside the app (partner companies and
//! kaprodi) who open the exported Excel without logging in.
//!
//! A link carries `{id_berkas}.{expiry}.{tag}`, where the tag is an HMAC-SHA256 over the first two
//! parts. It opens that one file until it expires, and changing any part of it breaks the tag.

use std::time::Duration;

use aws_lc_rs::hmac;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

/// Long enough for a round of selection by the partner companies.
pub const MASA_BERLAKU: Duration = Duration::from_secs(30 * 24 * 60 * 60);

const KONTEKS_KUNCI: &[u8] = b"gomagang/tautan-berkas/v1";

pub struct TautanBerkas {
    key: hmac::Key,
}

impl TautanBerkas {
    /// Membuat pembuat tautan publik dari `secret` (nilai `JWT_SECRET`).
    ///
    /// Kunci HMAC untuk tautan diturunkan dari secret dengan konteks khusus, sehingga tanda tangan
    /// tautan tidak pernah bisa disalahgunakan sebagai tanda tangan lain yang memakai secret yang
    /// sama,
    /// misalnya token sesi.
    pub fn new(secret: &str) -> Self {
        let induk = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
        let turunan = hmac::sign(&induk, KONTEKS_KUNCI);
        Self {
            key: hmac::Key::new(hmac::HMAC_SHA256, turunan.as_ref()),
        }
    }

    /// Membuat token tautan untuk berkas `id_berkas` yang berlaku sampai `kedaluwarsa` (detik
    /// Unix).
    ///
    /// Format token: `{id_berkas}.{kedaluwarsa}.{tag}`, dengan tag berupa HMAC-SHA256 atas dua
    /// bagian
    /// pertama dalam base64url tanpa padding.
    pub fn buat(&self, id_berkas: i32, kedaluwarsa: i64) -> String {
        let isi = format!("{id_berkas}.{kedaluwarsa}");
        let tag = hmac::sign(&self.key, isi.as_bytes());
        format!("{isi}.{}", URL_SAFE_NO_PAD.encode(tag.as_ref()))
    }

    /// Membaca token tautan dan mengembalikan `id_berkas` bila token sah pada waktu `sekarang`
    /// (detik
    /// Unix).
    ///
    /// Token sah bila dibuat oleh server ini, tidak diubah sedikit pun, dan belum kedaluwarsa.
    /// Selain
    /// itu, termasuk format yang rusak, hasilnya `None`.
    pub fn baca(&self, token: &str, sekarang: i64) -> Option<i32> {
        let (isi, tag) = token.rsplit_once('.')?;
        let tag = URL_SAFE_NO_PAD.decode(tag).ok()?;
        hmac::verify(&self.key, isi.as_bytes(), &tag).ok()?;

        let (id_berkas, kedaluwarsa) = isi.split_once('.')?;
        let kedaluwarsa: i64 = kedaluwarsa.parse().ok()?;
        (sekarang < kedaluwarsa)
            .then(|| id_berkas.parse().ok())
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret-with-at-least-32-bytes!";

    #[test]
    fn opens_only_the_same_file_until_it_expires() {
        let tautan = TautanBerkas::new(SECRET);
        let token = tautan.buat(12, 1_000);

        assert_eq!(tautan.baca(&token, 999), Some(12));
        assert_eq!(tautan.baca(&token, 1_000), None);
        assert_eq!(tautan.baca(&token, 5_000), None);
    }

    #[test]
    fn rejects_changed_or_foreign_tokens() {
        let tautan = TautanBerkas::new(SECRET);
        let token = tautan.buat(12, 1_000);
        let tag = token.rsplit_once('.').unwrap().1;

        for palsu in [
            format!("13.1000.{tag}"),
            format!("12.9999.{tag}"),
            format!("12.1000.{tag}x"),
            "12.1000".to_string(),
            "12".to_string(),
            String::new(),
            TautanBerkas::new("another-secret-with-at-least-32-bytes").buat(12, 1_000),
        ] {
            assert_eq!(tautan.baca(&palsu, 0), None, "{palsu}");
        }
    }
}
