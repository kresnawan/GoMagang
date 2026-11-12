//! Temporary way to register Panitia Magang until there is an admin flow. To remove it, delete this
//! folder, its line in `features/mod.rs`, and `admin_secret` in `Config` and `AppState`.

mod handler;
mod middleware;
mod model;
mod repository;
mod routes;
#[cfg(test)]
mod tests;

pub use routes::routes;
