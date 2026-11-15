//! Cara sementara untuk mendaftarkan Panitia Magang sampai ada alur admin. Untuk menghapusnya,
//! hapus folder ini, barisnya di `features/mod.rs`, serta `admin_secret` di `Config` dan
//! `AppState`.

mod handler;
mod middleware;
mod model;
mod repository;
mod routes;
#[cfg(test)]
mod tests;

pub use routes::routes;
