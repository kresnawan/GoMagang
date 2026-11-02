use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Form, Json, Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, Response, StatusCode, header},
    routing::post,
};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde_json::{Value, json};
use tower::ServiceExt;

use super::GoogleClient;
use crate::{app, config::Config, state::AppState};

const CLIENT_ID: &str = "test-client-id";
const CLIENT_SECRET: &str = "test-client-secret";
const FRONTEND_URL: &str = "http://frontend.test";
const JWT_SECRET: &str = "test-secret-with-at-least-32-bytes!";

fn config() -> Config {
    Config {
        cors_allowed_origins: vec![],
        google_client_id: CLIENT_ID.to_string(),
        google_client_secret: CLIENT_SECRET.to_string(),
        google_redirect_uri: "http://backend.test/auth/google".to_string(),
        frontend_url: FRONTEND_URL.to_string(),
        jwt_secret: JWT_SECRET.to_string(),
        cookie_secure: false,
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn sign(claims: &Value, secret: &str) -> String {
    let key = EncodingKey::from_secret(secret.as_bytes());
    encode(&Header::default(), claims, &key).unwrap()
}

fn id_token_claims() -> Value {
    json!({
        "sub": "1234567890",
        "email": "mahasiswa@student.ub.ac.id",
        "email_verified": true,
        "name": "Mahasiswa",
        "hd": "student.ub.ac.id",
        "aud": CLIENT_ID,
        "iss": "https://accounts.google.com",
        "exp": now() + 3600,
    })
}

struct MockGoogle {
    calls: Arc<AtomicUsize>,
    router: Router,
}

/// Serves `/token` on a local port. Only the code `good-code` is accepted.
async fn app_with_google(claims: Value) -> MockGoogle {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let id_token = sign(&claims, "google-key");
    let token_endpoint = move |Form(form): Form<HashMap<String, String>>| async move {
        counter.fetch_add(1, Ordering::SeqCst);
        let valid = form["code"] == "good-code"
            && form["client_id"] == CLIENT_ID
            && form["client_secret"] == CLIENT_SECRET
            && form["grant_type"] == "authorization_code"
            && form["redirect_uri"] == "http://backend.test/auth/google";
        if valid {
            (StatusCode::OK, Json(json!({ "id_token": id_token })))
        } else {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "invalid_grant" })),
            )
        }
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let mock = Router::new().route("/token", post(token_endpoint));
        axum::serve(listener, mock).await.unwrap();
    });

    let config = config();
    let mut state = AppState::new(&config);
    state.google =
        Arc::new(GoogleClient::new(&config).with_token_url(format!("http://{addr}/token")));

    MockGoogle {
        calls,
        router: app::build(&config, state),
    }
}

fn plain_app(config: &Config) -> Router {
    app::build(config, AppState::new(config))
}

async fn send(app: Router, method: &str, uri: &str, cookie: Option<&str>) -> Response<Body> {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    app.oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

fn location(response: &Response<Body>) -> &str {
    response.headers()[header::LOCATION].to_str().unwrap()
}

fn set_cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|value| value.to_str().unwrap())
        .find(|value| value.starts_with(&format!("{name}=")))
}

fn cookie_value(set_cookie: &str) -> &str {
    set_cookie.split(';').next().unwrap()
}

#[tokio::test]
async fn login_redirects_to_google_and_sets_state_cookie() {
    let response = send(plain_app(&config()), "GET", "/auth/google/login", None).await;

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let cookie = set_cookie(response.headers(), "oauth_state").unwrap();
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax"));
    assert!(cookie.contains("Path=/auth/google"));
    assert!(!cookie.contains("Secure"));

    let state = cookie_value(cookie).trim_start_matches("oauth_state=");
    assert_eq!(state.len(), 64);
    let url = location(&response);
    assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
    assert!(url.contains(&format!("state={state}")));
    assert!(url.contains(&format!("client_id={CLIENT_ID}")));
    assert!(url.contains("response_type=code"));
    assert!(url.contains("hd=student.ub.ac.id"));
    assert!(url.contains("redirect_uri=http%3A%2F%2Fbackend.test%2Fauth%2Fgoogle"));
}

#[tokio::test]
async fn login_uses_a_new_state_each_time() {
    let first = send(plain_app(&config()), "GET", "/auth/google/login", None).await;
    let second = send(plain_app(&config()), "GET", "/auth/google/login", None).await;

    assert_ne!(
        set_cookie(first.headers(), "oauth_state"),
        set_cookie(second.headers(), "oauth_state"),
    );
}

#[tokio::test]
async fn cookies_are_secure_when_configured() {
    let config = Config {
        cookie_secure: true,
        ..config()
    };
    let response = send(plain_app(&config), "GET", "/auth/google/login", None).await;

    let cookie = set_cookie(response.headers(), "oauth_state").unwrap();
    assert!(cookie.contains("Secure"));
}

#[tokio::test]
async fn callback_creates_a_session_that_me_accepts() {
    let mock = app_with_google(id_token_claims()).await;
    let response = send(
        mock.router.clone(),
        "GET",
        "/auth/google?code=good-code&state=abc",
        Some("oauth_state=abc"),
    )
    .await;

    assert_eq!(location(&response), "http://frontend.test/dasbor");
    let session = set_cookie(response.headers(), "session").unwrap();
    assert!(session.contains("HttpOnly"));
    assert!(session.contains("SameSite=Lax"));
    assert!(session.contains("Path=/"));
    assert!(session.contains("Max-Age=604800"));
    let state_cookie = set_cookie(response.headers(), "oauth_state").unwrap();
    assert!(state_cookie.contains("Max-Age=0"));

    let response = send(mock.router, "GET", "/auth/me", Some(cookie_value(session))).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let user: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        user,
        json!({
            "sub": "1234567890",
            "email": "mahasiswa@student.ub.ac.id",
            "name": "Mahasiswa",
        })
    );
}

#[tokio::test]
async fn callback_rejects_bad_state_without_calling_google() {
    let cases = [
        (
            "mismatch",
            "/auth/google?code=good-code&state=abc",
            Some("oauth_state=other"),
        ),
        ("no cookie", "/auth/google?code=good-code&state=abc", None),
        (
            "no param",
            "/auth/google?code=good-code",
            Some("oauth_state=abc"),
        ),
        ("neither", "/auth/google?code=good-code", None),
    ];

    for (name, uri, cookie) in cases {
        let mock = app_with_google(id_token_claims()).await;
        let response = send(mock.router, "GET", uri, cookie).await;

        assert_eq!(
            location(&response),
            "http://frontend.test/login?error=failed",
            "{name}"
        );
        assert!(
            set_cookie(response.headers(), "session").is_none(),
            "{name}"
        );
        assert_eq!(mock.calls.load(Ordering::SeqCst), 0, "{name}");
    }
}

#[tokio::test]
async fn callback_reports_cancelled_login() {
    let mock = app_with_google(id_token_claims()).await;
    let response = send(
        mock.router,
        "GET",
        "/auth/google?error=access_denied&state=abc",
        Some("oauth_state=abc"),
    )
    .await;

    assert_eq!(
        location(&response),
        "http://frontend.test/login?error=cancelled"
    );
    assert_eq!(mock.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn callback_rejects_code_google_refuses() {
    let mock = app_with_google(id_token_claims()).await;
    let response = send(
        mock.router,
        "GET",
        "/auth/google?code=bad-code&state=abc",
        Some("oauth_state=abc"),
    )
    .await;

    assert_eq!(
        location(&response),
        "http://frontend.test/login?error=failed"
    );
    assert!(set_cookie(response.headers(), "session").is_none());
}

async fn callback_location_with(field: &str, value: Option<Value>) -> String {
    let mut claims = id_token_claims();
    match value {
        Some(value) => claims[field] = value,
        None => {
            claims.as_object_mut().unwrap().remove(field);
        }
    }

    let mock = app_with_google(claims).await;
    let response = send(
        mock.router,
        "GET",
        "/auth/google?code=good-code&state=abc",
        Some("oauth_state=abc"),
    )
    .await;

    assert!(set_cookie(response.headers(), "session").is_none());
    location(&response).to_string()
}

#[tokio::test]
async fn callback_rejects_invalid_id_tokens() {
    let cases = [
        ("wrong aud", "aud", Some(json!("other-client"))),
        ("wrong iss", "iss", Some(json!("https://evil.example"))),
        ("expired", "exp", Some(json!(now() - 1))),
        ("missing aud", "aud", None),
        ("missing iss", "iss", None),
        ("missing exp", "exp", None),
        ("missing sub", "sub", None),
        ("missing email", "email", None),
    ];

    for (name, field, value) in cases {
        assert_eq!(
            callback_location_with(field, value).await,
            "http://frontend.test/login?error=failed",
            "{name}"
        );
    }
}

#[tokio::test]
async fn callback_rejects_disallowed_accounts() {
    let cases = [
        ("other domain", "email", Some(json!("a@gmail.com"))),
        (
            "suffix trick",
            "email",
            Some(json!("a@evilstudent.ub.ac.id")),
        ),
        ("parent domain", "email", Some(json!("a@ub.ac.id"))),
        ("unverified", "email_verified", Some(json!(false))),
        ("missing verified", "email_verified", None),
        ("other hd", "hd", Some(json!("gmail.com"))),
        ("missing hd", "hd", None),
    ];

    for (name, field, value) in cases {
        assert_eq!(
            callback_location_with(field, value).await,
            "http://frontend.test/login?error=domain",
            "{name}"
        );
    }
}

fn session_claims(exp: u64) -> Value {
    json!({ "sub": "1", "email": "a@student.ub.ac.id", "name": null, "iat": 0, "exp": exp })
}

#[tokio::test]
async fn me_rejects_missing_or_invalid_sessions() {
    let wrong_key = format!(
        "session={}",
        sign(
            &session_claims(now() + 3600),
            "another-secret-with-at-least-32-bytes"
        )
    );
    let expired = format!("session={}", sign(&session_claims(1000), JWT_SECRET));
    let cases = [
        ("missing", None),
        ("garbage", Some("session=not-a-jwt")),
        ("wrong key", Some(wrong_key.as_str())),
        ("expired", Some(expired.as_str())),
    ];

    for (name, cookie) in cases {
        let response = send(plain_app(&config()), "GET", "/auth/me", cookie).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{name}");
    }
}

#[tokio::test]
async fn logout_clears_the_session_cookie() {
    let response = send(
        plain_app(&config()),
        "POST",
        "/auth/logout",
        Some("session=anything"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let cookie = set_cookie(response.headers(), "session").unwrap();
    assert!(cookie.contains("Max-Age=0"));
    assert!(cookie.contains("Path=/"));
}
