//! Temporary way to fill the database with made-up mahasiswa and wipe all mahasiswa while trying
//! things out. To remove it, delete this folder, its line in `features/mod.rs`, and the export of
//! `require_admin_secret` in `features/tim_mb/mod.rs`.

mod dokumen;
mod handler;
mod model;
mod repository;
mod routes;
#[cfg(test)]
mod tests;

pub use routes::routes;
