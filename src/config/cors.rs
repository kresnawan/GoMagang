use axum::http::{
    HeaderValue, Method,
    header::{ACCEPT, AUTHORIZATION, CONTENT_DISPOSITION, CONTENT_TYPE},
};
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::logging::REQUEST_ID_HEADER;

/// Membuat layer CORS untuk frontend.
///
/// Hanya origin di `allowed_origins` (dari `CORS_ALLOWED_ORIGINS`) yang diizinkan, dengan metode
/// `GET`, `POST`, `PATCH`, `PUT`, dan `DELETE`. `allow_credentials` aktif karena sesi dikirim lewat
/// cookie. Header `Content-Disposition` dibuka (expose) agar frontend bisa membaca nama file saat
/// mengunduh berkas, misalnya surat keterangan, dan `X-Request-Id` agar frontend bisa menampilkan
/// ID request saat terjadi error.
pub fn layer(allowed_origins: &[HeaderValue]) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed_origins.iter().cloned()))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::PUT,
            Method::DELETE,
        ])
        .allow_headers([ACCEPT, AUTHORIZATION, CONTENT_TYPE])
        // Lets the frontend read the file name of a download, such as a surat keterangan, and the
        // request ID to quote when reporting an error.
        .expose_headers([CONTENT_DISPOSITION, REQUEST_ID_HEADER])
        .allow_credentials(true)
}
