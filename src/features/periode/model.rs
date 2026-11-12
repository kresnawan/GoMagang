use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::error::AppError;

const MAX_CATATAN_LEN: usize = 255;

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusPeriode {
    BelumDimulai,
    Berlangsung,
    Selesai,
}

impl StatusPeriode {
    /// Nama status seperti yang dipakai di API dan dihitung di query SQL: `belum_dimulai`,
    /// `berlangsung`, atau `selesai`.
    pub fn as_str(self) -> &'static str {
        match self {
            StatusPeriode::BelumDimulai => "belum_dimulai",
            StatusPeriode::Berlangsung => "berlangsung",
            StatusPeriode::Selesai => "selesai",
        }
    }
}

#[derive(Deserialize)]
pub struct PeriodeQuery {
    pub id: Option<i32>,
    pub status: Option<StatusPeriode>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct Periode {
    pub id_periode: i32,
    pub catatan: Option<String>,
    pub waktu_mulai: String,
    pub waktu_tutup: String,
    pub status: String,
}

/// What became of a request to start a period right away.
#[derive(Debug, PartialEq)]
pub enum StartResult {
    Started,
    NotFound,
    AlreadyRunning,
    Finished,
    Overlapping,
}

#[derive(Debug, FromRow)]
pub struct StartCheck {
    pub sudah_dimulai: bool,
    pub sudah_selesai: bool,
    pub beririsan: bool,
}

/// What became of a request to stop a period right away.
#[derive(Debug, PartialEq)]
pub enum StopResult {
    Stopped,
    NotFound,
    NotStarted,
    Finished,
}

#[derive(Debug, FromRow)]
pub struct StopCheck {
    pub sudah_dimulai: bool,
    pub sudah_selesai: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePeriode {
    pub catatan: Option<String>,
    pub waktu_mulai: String,
    pub waktu_tutup: String,
}

#[derive(Debug)]
pub struct NewPeriode {
    pub catatan: Option<String>,
    pub waktu_mulai: OffsetDateTime,
    pub waktu_tutup: OffsetDateTime,
}

impl CreatePeriode {
    /// Memvalidasi body pembuatan periode dan mengubahnya menjadi [`NewPeriode`].
    ///
    /// - `catatan`: spasi di awal dan akhir dibuang; teks kosong dianggap tidak ada; maksimal 255
    ///   karakter.
    /// - `waktu_mulai` dan `waktu_tutup`: format ISO 8601 dengan zona waktu (lihat [`parse_time`]).
    /// - `waktu_tutup` harus setelah `waktu_mulai`.
    ///
    /// Pelanggaran dijawab [`AppError::Validation`].
    pub fn validate(self) -> Result<NewPeriode, AppError> {
        let catatan = self
            .catatan
            .map(|catatan| catatan.trim().to_string())
            .filter(|catatan| !catatan.is_empty());
        if catatan
            .as_ref()
            .is_some_and(|catatan| catatan.chars().count() > MAX_CATATAN_LEN)
        {
            return Err(AppError::Validation("Catatan maksimal 255 karakter."));
        }

        let waktu_mulai = parse_time(&self.waktu_mulai)?;
        let waktu_tutup = parse_time(&self.waktu_tutup)?;
        if waktu_tutup <= waktu_mulai {
            return Err(AppError::Validation(
                "Waktu tutup harus setelah waktu mulai.",
            ));
        }

        Ok(NewPeriode {
            catatan,
            waktu_mulai,
            waktu_tutup,
        })
    }
}

/// Membaca waktu berformat RFC 3339 / ISO 8601 yang wajib menyertakan zona waktu, misalnya
/// `2026-10-01T08:00:00+07:00`.
///
/// Zona waktu diwajibkan agar waktu yang dikirim frontend tidak ditafsirkan berbeda oleh server.
/// Format lain dijawab [`AppError::Validation`] dengan contoh format yang benar.
fn parse_time(value: &str) -> Result<OffsetDateTime, AppError> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(|_| {
        AppError::Validation(
            "Waktu harus berformat ISO 8601 dengan zona waktu, misalnya 2026-10-01T08:00:00+07:00.",
        )
    })
}
