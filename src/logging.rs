//! Logging terstruktur: satu objek JSON per baris di stdout, satu baris log untuk setiap request,
//! dan request ID yang mengaitkan semua log selama satu request diproses.

use std::time::Duration;

use axum::{
    Router,
    body::Body,
    extract::{MatchedPath, Request},
    http::{HeaderName, Response},
    middleware,
};
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};
use tracing::{Span, field};
use tracing_subscriber::EnvFilter;

use crate::state::AppState;

/// Dipakai bila `RUST_LOG` tidak di-set: semua request dan semua log aplikasi ini, tanpa log
/// internal library seperti sqlx dan hyper.
const DEFAULT_FILTER: &str = "gomagang=info,tower_http=info";

pub const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

/// Request ID dari luar (biasanya dari reverse proxy) yang lebih panjang dari ini diganti dengan ID
/// baru.
const MAX_REQUEST_ID_LEN: usize = 128;

/// Memasang logger global yang menulis satu objek JSON per baris ke stdout.
///
/// Level dan target diatur lewat `RUST_LOG` dengan sintaks `EnvFilter`, misalnya
/// `RUST_LOG=gomagang=debug,sqlx=warn`. Bila `RUST_LOG` tidak di-set atau kosong, dipakai
/// [`DEFAULT_FILTER`]. Field sebuah log ada di tingkat atas objek JSON, dan field request yang
/// sedang berjalan (termasuk `request_id`) ada di `span`.
///
/// # Panics
/// Panic bila `RUST_LOG` di-set tetapi sintaksnya tidak valid, atau bila logger global sudah
/// terpasang sebelumnya.
///
/// Panic hook juga diganti agar panic, termasuk panic di dalam handler, tercatat sebagai log
/// `ERROR` berformat JSON dengan pesan dan lokasinya, bukan teks biasa di stderr.
pub fn init() {
    let filter = match std::env::var("RUST_LOG") {
        Ok(value) if !value.trim().is_empty() => EnvFilter::try_new(&value)
            .unwrap_or_else(|err| panic!("invalid RUST_LOG {value:?}: {err}")),
        _ => EnvFilter::new(DEFAULT_FILTER),
    };

    tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .with_writer(std::io::stdout)
        .flatten_event(true)
        .with_current_span(true)
        .with_span_list(false)
        .init();

    std::panic::set_hook(Box::new(|info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("panic without a message");
        let location = info.location().map(ToString::to_string);
        tracing::error!(panic_message = message, location, "panic");
    }));
}

/// Memasang logging request dan request ID pada router.
///
/// Urutan dari luar ke dalam:
/// 1. Header `X-Request-Id` dari luar yang tidak wajar dibuang (lihat [`is_valid_request_id`]).
/// 2. Bila request belum membawa `X-Request-Id`, dibuatkan UUID v4. ID dari reverse proxy yang
///    valid dipakai apa adanya, sehingga log proxy dan backend bisa dicocokkan.
/// 3. Setiap request dibungkus span `request` berisi `request_id`, `method`, dan `route`, lalu
///    dicatat satu log saat respons selesai (lihat [`on_response`]).
/// 4. `X-Request-Id` dikirim balik di header respons.
///
/// `route` berisi pola route, misalnya `/api/v1/berkas/publik/{token}`, bukan path aslinya,
/// sehingga token di path dan query string tidak pernah masuk log. Path asli hanya dicatat (di
/// field `path`) untuk request yang tidak cocok dengan route mana pun.
///
/// Layer ini dipasang lewat `Router::layer` agar [`MatchedPath`] sudah tersedia saat span dibuat.
pub fn layer(router: Router<AppState>) -> Router<AppState> {
    router
        .layer(PropagateRequestIdLayer::new(REQUEST_ID_HEADER))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(make_span)
                .on_request(())
                .on_response(on_response)
                .on_failure(()),
        )
        .layer(SetRequestIdLayer::new(REQUEST_ID_HEADER, MakeRequestUuid))
        .layer(middleware::map_request(drop_invalid_request_id))
}

/// Membuat span untuk satu request. Semua log selama request ini membawa field span-nya.
fn make_span(request: &Request<Body>) -> Span {
    let request_id = request
        .headers()
        .get(&REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let route = request.extensions().get::<MatchedPath>();

    let span = tracing::info_span!(
        "request",
        request_id,
        method = %request.method(),
        route = route.map(MatchedPath::as_str),
        path = field::Empty,
    );
    if route.is_none() {
        span.record("path", request.uri().path());
    }
    span
}

/// Mencatat satu log per request yang selesai, berisi `status` dan `latency_ms`.
///
/// Respons `5xx` dicatat di level `ERROR`, selain itu `INFO`. `4xx` sengaja tidak `WARN`, karena
/// hal biasa seperti access token yang kedaluwarsa (`401`) akan membanjiri peringatan.
fn on_response(response: &Response<Body>, latency: Duration, _span: &Span) {
    let status = response.status().as_u16();
    let latency_ms = u64::try_from(latency.as_millis()).unwrap_or(u64::MAX);

    if response.status().is_server_error() {
        tracing::error!(status, latency_ms, "request completed");
    } else {
        tracing::info!(status, latency_ms, "request completed");
    }
}

/// Membuang header `X-Request-Id` dari luar yang tidak lolos [`is_valid_request_id`], sehingga
/// server membuatkan ID baru untuknya.
async fn drop_invalid_request_id(mut request: Request) -> Request {
    let invalid = request
        .headers()
        .get_all(&REQUEST_ID_HEADER)
        .iter()
        .enumerate()
        .any(|(i, value)| i > 0 || !is_valid_request_id(value.as_bytes()));
    if invalid {
        request.headers_mut().remove(&REQUEST_ID_HEADER);
    }
    request
}

/// Request ID dari luar diterima bila tidak kosong, paling panjang [`MAX_REQUEST_ID_LEN`] byte,
/// dan hanya berisi huruf, angka, `-`, `_`, `.`, atau `:`. Batasan ini mencegah klien mengisi log
/// dengan teks panjang atau karakter yang membingungkan pembaca log.
fn is_valid_request_id(value: &[u8]) -> bool {
    !value.is_empty()
        && value.len() <= MAX_REQUEST_ID_LEN
        && value
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

#[cfg(test)]
mod tests {
    use axum::http::{Request, StatusCode, header};
    use sqlx::PgPool;
    use tower::ServiceExt;

    use super::*;
    use crate::{app, config::Config};

    fn setup(db: PgPool) -> Router {
        let config = Config {
            database_url: String::new(),
            cors_allowed_origins: vec!["http://frontend.test".parse().unwrap()],
            google_client_id: "client".to_string(),
            google_client_secret: "secret".to_string(),
            google_redirect_uri: "http://backend.test/auth/google".to_string(),
            frontend_url: "http://frontend.test".to_string(),
            jwt_secret: "test-secret-with-at-least-32-bytes!".to_string(),
            admin_secret: None,
            cookie_secure: false,
            upload_dir: Default::default(),
            surat_signing_key: None,
            smtp: None,
        };
        let state = AppState::new(&config, db);
        app::build(&config, state)
    }

    async fn request_id_for(app: Router, uri: &str, sent: &[&str]) -> String {
        let mut request = Request::get(uri).header(header::ORIGIN, "http://frontend.test");
        for value in sent {
            request = request.header(&REQUEST_ID_HEADER, *value);
        }
        let response = app
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        response
            .headers()
            .get(&REQUEST_ID_HEADER)
            .expect("response carries X-Request-Id")
            .to_str()
            .unwrap()
            .to_string()
    }

    fn is_uuid(value: &str) -> bool {
        uuid::Uuid::parse_str(value).is_ok()
    }

    #[sqlx::test]
    async fn generates_request_id_when_absent(db: PgPool) {
        let id = request_id_for(setup(db), "/health", &[]).await;

        assert!(is_uuid(&id), "{id} is not a UUID");
    }

    #[sqlx::test]
    async fn every_request_gets_a_different_id(db: PgPool) {
        let app = setup(db);
        let first = request_id_for(app.clone(), "/health", &[]).await;
        let second = request_id_for(app, "/health", &[]).await;

        assert_ne!(first, second);
    }

    #[sqlx::test]
    async fn keeps_valid_request_id_from_proxy(db: PgPool) {
        let id = request_id_for(setup(db), "/health", &["proxy-abc_123.4:5"]).await;

        assert_eq!(id, "proxy-abc_123.4:5");
    }

    #[sqlx::test]
    async fn replaces_invalid_request_id(db: PgPool) {
        let app = setup(db);
        let too_long = "a".repeat(MAX_REQUEST_ID_LEN + 1);

        for sent in ["has space", "<script>", too_long.as_str()] {
            let id = request_id_for(app.clone(), "/health", &[sent]).await;
            assert!(is_uuid(&id), "{sent:?} was kept as {id}");
        }
    }

    #[sqlx::test]
    async fn replaces_duplicate_request_id(db: PgPool) {
        let id = request_id_for(setup(db), "/health", &["first", "second"]).await;

        assert!(is_uuid(&id), "duplicate header was kept as {id}");
    }

    #[sqlx::test]
    async fn request_id_on_error_and_unmatched_responses(db: PgPool) {
        let app = setup(db);

        // 401 from an auth extractor, and 404 from no route at all.
        assert!(is_uuid(&request_id_for(app.clone(), "/auth/me", &[]).await));
        assert!(is_uuid(&request_id_for(app, "/no/such/route", &[]).await));
    }

    #[sqlx::test]
    async fn cors_exposes_request_id(db: PgPool) {
        let response = setup(db)
            .oneshot(
                Request::get("/health")
                    .header(header::ORIGIN, "http://frontend.test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let exposed = response
            .headers()
            .get(header::ACCESS_CONTROL_EXPOSE_HEADERS)
            .unwrap()
            .to_str()
            .unwrap()
            .to_ascii_lowercase();
        assert!(exposed.contains("x-request-id"), "exposed: {exposed}");
    }

    #[test]
    fn request_id_rules() {
        assert!(is_valid_request_id(b"7f3c2a10-1b2c-4d5e-8f90-0123456789ab"));
        assert!(!is_valid_request_id(b""));
        assert!(!is_valid_request_id(b"a\"b"));
        assert!(!is_valid_request_id(&[b'a'; MAX_REQUEST_ID_LEN + 1]));
        assert!(is_valid_request_id(&[b'a'; MAX_REQUEST_ID_LEN]));
    }
}
