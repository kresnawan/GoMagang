use axum::http::{
    HeaderValue, Method,
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
};
use tower_http::cors::{AllowOrigin, CorsLayer};

pub fn layer(allowed_origins: &[HeaderValue]) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed_origins.iter().cloned()))
        .allow_methods([Method::GET, Method::POST, Method::PATCH])
        .allow_headers([ACCEPT, AUTHORIZATION, CONTENT_TYPE])
        .allow_credentials(true)
}
