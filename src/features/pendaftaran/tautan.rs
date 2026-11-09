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
    /// The key is derived from `secret` for links only, so a link can never pass for anything
    /// else signed with the same secret, such as a session token.
    pub fn new(secret: &str) -> Self {
        let induk = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
        let turunan = hmac::sign(&induk, KONTEKS_KUNCI);
        Self {
            key: hmac::Key::new(hmac::HMAC_SHA256, turunan.as_ref()),
        }
    }

    /// A token for this file that works until `kedaluwarsa`, in Unix seconds.
    pub fn buat(&self, id_berkas: i32, kedaluwarsa: i64) -> String {
        let isi = format!("{id_berkas}.{kedaluwarsa}");
        let tag = hmac::sign(&self.key, isi.as_bytes());
        format!("{isi}.{}", URL_SAFE_NO_PAD.encode(tag.as_ref()))
    }

    /// The id_berkas, if the token was made here, is unchanged, and has not expired at `sekarang`.
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
