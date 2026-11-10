use axum::http::{
    HeaderValue, Method,
    header::{ACCEPT, AUTHORIZATION, CONTENT_DISPOSITION, CONTENT_TYPE},
};
use tower_http::cors::{AllowOrigin, CorsLayer};

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
        // Lets the frontend read the file name of a download, such as a surat keterangan.
        .expose_headers([CONTENT_DISPOSITION])
        .allow_credentials(true)
}
