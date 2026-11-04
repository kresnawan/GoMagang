use serde::{Deserialize, Serialize};

use crate::error::AppError;

const GMAIL_SUFFIX: &str = "@gmail.com";
const MAX_EMAIL_LEN: usize = 255;
const MAX_NAMA_LEN: usize = 255;
const NO_HP_DIGITS: std::ops::RangeInclusive<usize> = 9..=15;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterTimMb {
    pub email: String,
    pub nama: Option<String>,
    pub no_hp: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TimMb {
    pub id_pengguna: i32,
    pub email: String,
    pub nama: Option<String>,
    pub no_hp: Option<String>,
}

impl RegisterTimMb {
    pub fn validate(mut self) -> Result<Self, AppError> {
        self.email = self.email.trim().to_ascii_lowercase();
        let local = self.email.strip_suffix(GMAIL_SUFFIX).unwrap_or_default();
        if local.is_empty()
            || self.email.len() > MAX_EMAIL_LEN
            || local.contains('@')
            || local.contains(char::is_whitespace)
        {
            return Err(AppError::Validation(
                "Email harus berupa alamat @gmail.com, karena Tim MB login dengan Gmail.",
            ));
        }
        if let Some(nama) = &self.nama {
            let nama = nama.trim();
            if nama.is_empty() || nama.chars().count() > MAX_NAMA_LEN {
                return Err(AppError::Validation("Nama harus berisi 1-255 karakter."));
            }
            self.nama = Some(nama.to_string());
        }
        if let Some(no_hp) = &self.no_hp {
            let digits = no_hp.strip_prefix('+').unwrap_or(no_hp);
            let valid = NO_HP_DIGITS.contains(&digits.len())
                && digits.bytes().all(|byte| byte.is_ascii_digit());
            if !valid {
                return Err(AppError::Validation(
                    "Nomor HP harus berupa 9-15 digit angka, boleh diawali +.",
                ));
            }
        }

        Ok(self)
    }
}
