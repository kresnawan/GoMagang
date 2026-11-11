//! Temporary way to register Panitia Magang until there is an admin flow. To remove it, first remove
//! `features/data_uji_coba`, which uses the same middleware, then delete this folder, its line in
//! `features/mod.rs`, and `admin_secret` in `Config` and `AppState`.

mod handler;
mod middleware;
mod model;
mod repository;
mod routes;
#[cfg(test)]
mod tests;

pub use middleware::require_admin_secret;
pub use routes::routes;
