//! Health check untuk monitoring: apakah server hidup dan bisa menjangkau database.

mod handler;
mod routes;
#[cfg(test)]
mod tests;

pub use routes::routes;
