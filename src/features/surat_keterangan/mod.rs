//! Surat keterangan kepesertaan magang: generated from a template on the first download of an
//! Aktif pendaftaran, signed with Ed25519, and verifiable by anyone through its QR code or by
//! uploading the PDF.

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
