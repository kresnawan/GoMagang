use aws_lc_rs::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};

/// Prefixed to everything signed, so that a signature made here cannot be passed off as one over
/// some other kind of data signed with the same key.
const KONTEKS: &[u8] = b"gomagang/surat-keterangan/v1\n";

/// Signs and checks surat keterangan with the Ed25519 key from SURAT_SIGNING_KEY. Only the server
/// holds the key, so nobody else can produce a signature that passes `cocok`.
pub struct PenandaSurat {
    kunci: Ed25519KeyPair,
}

impl PenandaSurat {
    /// Panics on a malformed key, like the other secrets that are checked at startup.
    pub fn from_base64_seed(seed: &str) -> Self {
        let seed = STANDARD
            .decode(seed.trim())
            .expect("SURAT_SIGNING_KEY must be base64");
        assert_eq!(seed.len(), 32, "SURAT_SIGNING_KEY must decode to 32 bytes");
        let kunci = Ed25519KeyPair::from_seed_unchecked(&seed)
            .expect("SURAT_SIGNING_KEY is not a valid Ed25519 seed");
        Self { kunci }
    }

    /// base64url signature, without padding, over `isi`.
    pub fn tandatangani(&self, isi: &str) -> String {
        URL_SAFE_NO_PAD.encode(self.kunci.sign(&pesan(isi)).as_ref())
    }

    pub fn cocok(&self, isi: &str, tanda_tangan: &str) -> bool {
        let Ok(tanda_tangan) = URL_SAFE_NO_PAD.decode(tanda_tangan) else {
            return false;
        };
        UnparsedPublicKey::new(&ED25519, self.kunci.public_key().as_ref())
            .verify(&pesan(isi), &tanda_tangan)
            .is_ok()
    }

    /// base64 public key, for anyone who wants to check a signature without asking the server.
    pub fn kunci_publik(&self) -> String {
        STANDARD.encode(self.kunci.public_key().as_ref())
    }
}

fn pesan(isi: &str) -> Vec<u8> {
    [KONTEKS, isi.as_bytes()].concat()
}

/// The code carried by the QR: `{id_surat}.{tanda_tangan}`. The id finds the row, and the
/// signature proves the code was issued here rather than guessed.
pub fn kode(id_surat: i32, tanda_tangan: &str) -> String {
    format!("{id_surat}.{tanda_tangan}")
}

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
