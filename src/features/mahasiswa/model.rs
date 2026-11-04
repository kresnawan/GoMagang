use serde::{Deserialize, Deserializer, Serialize};
use sqlx::FromRow;

use crate::error::AppError;

const PROGRAM_STUDI: [&str; 5] = [
    "Sistem Informasi",
    "Teknologi Informasi",
    "Pendidikan Teknologi Informasi",
    "Teknik Komputer",
    "Teknik Informatika",
];
const MAX_NIM_LEN: usize = 15;
const MAX_NAMA_LEN: usize = 255;
const NO_HP_DIGITS: std::ops::RangeInclusive<usize> = 9..=15;
const IPK_RANGE: std::ops::RangeInclusive<f64> = 0.0..=4.0;
const SEMESTER_RANGE: std::ops::RangeInclusive<i16> = 1..=14;

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct Mahasiswa {
    pub nim: Option<String>,
    pub email: String,
    pub nama: Option<String>,
    pub program_studi: Option<String>,
    pub ipk: Option<f64>,
    pub no_hp: Option<String>,
    pub jenis_kelamin: Option<String>,
    pub semester: Option<i16>,
}

/// The columns a mahasiswa can change. A field that is left out keeps its current value.
///
/// Sending `null` is rejected, and so are other fields such as `email`, so that a mistake in the
/// body fails loudly instead of being ignored.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MahasiswaPatch {
    #[serde(default, deserialize_with = "present")]
    pub nim: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub nama: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub program_studi: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub ipk: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    pub no_hp: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub jenis_kelamin: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub semester: Option<i16>,
}

fn present<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

impl MahasiswaPatch {
    pub fn validate(mut self) -> Result<Self, AppError> {
        if let Some(nim) = &self.nim
            && !(is_digits(nim) && nim.len() <= MAX_NIM_LEN)
        {
            return Err(AppError::Validation("NIM harus berupa 1-15 digit angka."));
        }
        if let Some(nama) = &self.nama {
            let nama = nama.trim();
            if nama.is_empty() || nama.chars().count() > MAX_NAMA_LEN {
                return Err(AppError::Validation("Nama harus berisi 1-255 karakter."));
            }
            self.nama = Some(nama.to_string());
        }
        if let Some(program_studi) = &self.program_studi
            && !PROGRAM_STUDI.contains(&program_studi.as_str())
        {
            return Err(AppError::Validation("Program studi tidak valid."));
        }
        if let Some(ipk) = self.ipk
            && !IPK_RANGE.contains(&ipk)
        {
            return Err(AppError::Validation("IPK harus di antara 0 dan 4."));
        }
        if let Some(no_hp) = &self.no_hp {
            let digits = no_hp.strip_prefix('+').unwrap_or(no_hp);
            if !(is_digits(digits) && NO_HP_DIGITS.contains(&digits.len())) {
                return Err(AppError::Validation(
                    "Nomor HP harus berupa 9-15 digit angka, boleh diawali +.",
                ));
            }
        }
        if let Some(jenis_kelamin) = &self.jenis_kelamin
            && !matches!(jenis_kelamin.as_str(), "L" | "P")
        {
            return Err(AppError::Validation("Jenis kelamin harus L atau P."));
        }
        if let Some(semester) = self.semester
            && !SEMESTER_RANGE.contains(&semester)
        {
            return Err(AppError::Validation("Semester harus di antara 1 dan 14."));
        }

        Ok(self)
    }
}

fn is_digits(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}
