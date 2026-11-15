//! Surat keterangan kepesertaan magang: dibuat dari templat saat pendaftaran `Aktif` pertama kali
//! diunduh suratnya, ditandatangani dengan Ed25519, dan bisa diverifikasi siapa pun lewat QR
//! code-nya atau dengan mengunggah PDF-nya.

mod dokumen;
mod handler;
mod huruf;
mod model;
mod repository;
mod routes;
mod tanda_tangan;
mod templat;
#[cfg(test)]
mod tests;

pub use routes::routes;
pub use tanda_tangan::PenandaSurat;
