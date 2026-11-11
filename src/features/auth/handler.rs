use std::time::Duration;

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};

use super::{
    google::AccountKind,
    model::{GoogleCallbackQuery, GoogleClaims, User},
    repository,
    session::{REFRESH_COOKIE, REFRESH_GRACE, REFRESH_TTL, generate_token, hash_token},
};
use crate::{error::AppError, state::AppState};

const STATE_COOKIE: &str = "oauth_state";
const STATE_COOKIE_PATH: &str = "/auth/google";
const STATE_TTL: Duration = Duration::from_secs(10 * 60);
const MAHASISWA_HOME_PATH: &str = "/dasbor";
const PANITIA_MAGANG_HOME_PATH: &str = "/panitia-magang";

pub async fn google_login(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, Redirect), AppError> {
    let oauth_state = generate_token();
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
            let url = format!("{}{}", state.frontend_url, session.home_path);
            (
                jar.add(session.access).add(session.refresh),
                Redirect::to(&url),
            )
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

pub async fn refresh(State(state): State<AppState>, jar: CookieJar) -> Response {
    match rotate_session(&state, &jar).await {
        Ok((access, refresh)) => {
            (jar.add(access).add(refresh), StatusCode::NO_CONTENT).into_response()
        }
        Err(AppError::Unauthorized) => {
            (state.sessions.clear_cookies(jar), StatusCode::UNAUTHORIZED).into_response()
        }
        // Keep the cookies, the session may still be valid once the failure is over.
        Err(err) => err.into_response(),
    }
}

pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, StatusCode), AppError> {
    if let Some(cookie) = jar.get(REFRESH_COOKIE) {
        repository::revoke_family(&state.db, &hash_token(cookie.value())).await?;
    }

    Ok((state.sessions.clear_cookies(jar), StatusCode::NO_CONTENT))
}

struct LoginSession {
    access: Cookie<'static>,
    refresh: Cookie<'static>,
    home_path: &'static str,
}

/// Returns the new session, or the error code the frontend expects on `/login?error=`.
async fn complete_login(
    state: &AppState,
    expected_state: Option<String>,
    query: GoogleCallbackQuery,
) -> Result<LoginSession, &'static str> {
    if expected_state.is_none() || expected_state != query.state {
        return Err("failed");
    }
    if query.error.is_some() {
        return Err("cancelled");
    }

    let code = query.code.ok_or("failed")?;
    let (claims, kind) = state
        .google
        .exchange_code(&code)
        .await
        .map_err(|err| match err {
            AppError::Forbidden => "domain",
            _ => "failed",
        })?;

    start_session(state, &claims, kind)
        .await
        .map_err(|err| match err {
            AppError::Forbidden => "not_registered",
            _ => "failed",
        })
}

async fn start_session(
    state: &AppState,
    claims: &GoogleClaims,
    kind: AccountKind,
) -> Result<LoginSession, AppError> {
    let (user, home_path) = match kind {
        AccountKind::Mahasiswa => (
            repository::upsert_mahasiswa(&state.db, claims).await?,
            MAHASISWA_HOME_PATH,
        ),
        AccountKind::PanitiaMagang => (
            repository::find_panitia_magang(&state.db, claims)
                .await?
                .ok_or(AppError::Forbidden)?,
            PANITIA_MAGANG_HOME_PATH,
        ),
    };
    let token = generate_token();
    repository::create_refresh_token(
        &state.db,
        user.id_pengguna,
        &hash_token(&token),
        REFRESH_TTL,
    )
    .await?;

    Ok(LoginSession {
        access: state.sessions.access_cookie(&user)?,
        refresh: state.sessions.refresh_cookie(token, REFRESH_TTL)?,
        home_path,
    })
}

async fn rotate_session(
    state: &AppState,
    jar: &CookieJar,
) -> Result<(Cookie<'static>, Cookie<'static>), AppError> {
    let cookie = jar.get(REFRESH_COOKIE).ok_or(AppError::Unauthorized)?;
    let new_token = generate_token();
    let rotated = repository::rotate_refresh_token(
        &state.db,
        &hash_token(cookie.value()),
        &hash_token(&new_token),
        REFRESH_GRACE,
    )
    .await?
    .ok_or(AppError::Unauthorized)?;

    Ok((
        state.sessions.access_cookie(&rotated.user)?,
        state
            .sessions
            .refresh_cookie(new_token, rotated.expires_in)?,
    ))
}
