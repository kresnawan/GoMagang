use std::time::Duration;

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::Redirect,
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};

use super::model::{GoogleCallbackQuery, User};
use crate::{error::AppError, state::AppState};

const STATE_COOKIE: &str = "oauth_state";
const STATE_COOKIE_PATH: &str = "/auth/google";
const STATE_TTL: Duration = Duration::from_secs(10 * 60);
const LOGIN_SUCCESS_PATH: &str = "/dasbor";

pub async fn google_login(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, Redirect), AppError> {
    let oauth_state = generate_state();
    let cookie = Cookie::build((STATE_COOKIE, oauth_state.clone()))
        .path(STATE_COOKIE_PATH)
        .http_only(true)
        // Lax, because the cookie has to come back on the top-level redirect from Google.
        .same_site(SameSite::Lax)
        .secure(state.cookie_secure)
        .max_age(STATE_TTL.try_into().map_err(|_| AppError::Internal)?);
    let url = state.google.authorization_url(&oauth_state);

    Ok((jar.add(cookie), Redirect::to(&url)))
}

pub async fn google_callback(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<GoogleCallbackQuery>,
) -> (CookieJar, Redirect) {
    let expected_state = jar.get(STATE_COOKIE).map(|c| c.value().to_string());
    let jar = jar.remove(Cookie::build(STATE_COOKIE).path(STATE_COOKIE_PATH));

    match complete_login(&state, expected_state, query).await {
        Ok(session) => {
            let url = format!("{}{LOGIN_SUCCESS_PATH}", state.frontend_url);
            (jar.add(session), Redirect::to(&url))
        }
        Err(code) => {
            let url = format!("{}/login?error={code}", state.frontend_url);
            (jar, Redirect::to(&url))
        }
    }
}

pub async fn me(user: User) -> Json<User> {
    Json(user)
}

pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> (CookieJar, StatusCode) {
    (
        jar.remove(state.sessions.removal_cookie()),
        StatusCode::NO_CONTENT,
    )
}

/// Returns the session cookie, or the error code the frontend expects on `/login?error=`.
async fn complete_login(
    state: &AppState,
    expected_state: Option<String>,
    query: GoogleCallbackQuery,
) -> Result<Cookie<'static>, &'static str> {
    if expected_state.is_none() || expected_state != query.state {
        return Err("failed");
    }
    if query.error.is_some() {
        return Err("cancelled");
    }

    let code = query.code.ok_or("failed")?;
    let claims = state
        .google
        .exchange_code(&code)
        .await
        .map_err(|err| match err {
            AppError::Forbidden => "domain",
            _ => "failed",
        })?;

    state
        .sessions
        .create_cookie(claims.into())
        .map_err(|_| "failed")
}

fn generate_state() -> String {
    rand::random::<[u8; 32]>()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
