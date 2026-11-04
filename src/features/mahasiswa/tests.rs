use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, Response, StatusCode, header},
};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

use crate::{app, config::Config, features::auth::User, state::AppState};

const JWT_SECRET: &str = "test-secret-with-at-least-32-bytes!";
const URI: &str = "/api/v1/mahasiswa/data";

fn config() -> Config {
    Config {
        database_url: String::new(),
        cors_allowed_origins: vec![],
        google_client_id: "client".to_string(),
        google_client_secret: "secret".to_string(),
        google_redirect_uri: "http://backend.test/auth/google".to_string(),
        frontend_url: "http://frontend.test".to_string(),
        jwt_secret: JWT_SECRET.to_string(),
        admin_secret: None,
        cookie_secure: false,
    }
}

async fn insert_pengguna(db: &PgPool, email: &str) -> User {
    let id_pengguna = sqlx::query_scalar(
        "INSERT INTO pengguna (email, google_sub) VALUES ($1, $1) RETURNING id_pengguna",
    )
    .bind(email)
    .fetch_one(db)
    .await
    .unwrap();

    User {
        id_pengguna,
        email: email.to_string(),
    }
}

async fn insert_mahasiswa(db: &PgPool, user: &User, nim: &str) {
    sqlx::query(
        "INSERT INTO mahasiswa
             (nim, id_pengguna, nama, program_studi, ipk, no_hp, jenis_kelamin, semester)
         VALUES ($1, $2, 'Budi', 'Teknik Informatika', 3.75, '0812', 'L', 14)",
    )
    .bind(nim)
    .bind(user.id_pengguna)
    .execute(db)
    .await
    .unwrap();
}

fn access_cookie(state: &AppState, user: &User) -> String {
    let cookie = state.sessions.access_cookie(user).unwrap();
    format!("{}={}", cookie.name(), cookie.value())
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

async fn json_body(response: Response<Body>) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn setup(db: PgPool) -> (Router, AppState) {
    let config = config();
    let state = AppState::new(&config, db);
    (app::build(&config, state.clone()), state)
}

#[sqlx::test]
async fn returns_the_data_of_the_token_owner_only(db: PgPool) {
    let (app, state) = setup(db.clone());
    let budi = insert_pengguna(&db, "budi@student.ub.ac.id").await;
    let ani = insert_pengguna(&db, "ani@student.ub.ac.id").await;
    insert_mahasiswa(&db, &budi, "215150200111001").await;
    insert_mahasiswa(&db, &ani, "215150200111002").await;

    let cookie = access_cookie(&state, &ani);
    let response = send(app, "GET", URI, Some(&cookie)).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await,
        json!({
            "nim": "215150200111002",
            "email": "ani@student.ub.ac.id",
            "nama": "Budi",
            "program_studi": "Teknik Informatika",
            "ipk": 3.75,
            "no_hp": "0812",
            "jenis_kelamin": "L",
            "semester": 14,
        })
    );
}

#[sqlx::test]
async fn returns_null_for_fields_not_filled_in_yet(db: PgPool) {
    let (app, state) = setup(db.clone());
    let user = insert_pengguna(&db, "a@student.ub.ac.id").await;
    sqlx::query("INSERT INTO mahasiswa (id_pengguna) VALUES ($1)")
        .bind(user.id_pengguna)
        .execute(&db)
        .await
        .unwrap();

    let cookie = access_cookie(&state, &user);
    let response = send(app, "GET", URI, Some(&cookie)).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await,
        json!({
            "nim": null,
            "email": "a@student.ub.ac.id",
            "nama": null,
            "program_studi": null,
            "ipk": null,
            "no_hp": null,
            "jenis_kelamin": null,
            "semester": null,
        })
    );
}

#[sqlx::test]
async fn drops_the_char_padding_from_nim(db: PgPool) {
    let (app, state) = setup(db.clone());
    let user = insert_pengguna(&db, "a@student.ub.ac.id").await;
    insert_mahasiswa(&db, &user, "123").await;

    let cookie = access_cookie(&state, &user);
    let body = json_body(send(app, "GET", URI, Some(&cookie)).await).await;

    assert_eq!(body["nim"], "123");
}

#[sqlx::test]
async fn forbids_a_valid_token_without_a_mahasiswa_row(db: PgPool) {
    let (app, state) = setup(db.clone());
    let user = insert_pengguna(&db, "a@student.ub.ac.id").await;

    let cookie = access_cookie(&state, &user);
    let response = send(app, "GET", URI, Some(&cookie)).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn forbids_a_valid_token_of_a_deleted_pengguna(db: PgPool) {
    let (app, state) = setup(db);
    let user = User {
        id_pengguna: 999,
        email: "a@student.ub.ac.id".to_string(),
    };

    let cookie = access_cookie(&state, &user);
    let response = send(app, "GET", URI, Some(&cookie)).await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

fn token(sub: &str, exp: u64, secret: &str) -> String {
    let claims = json!({ "sub": sub, "email": "a@student.ub.ac.id", "iat": 0, "exp": exp });
    let key = EncodingKey::from_secret(secret.as_bytes());
    let token = encode(&Header::default(), &claims, &key).unwrap();
    format!("access_token={token}")
}

#[sqlx::test]
async fn rejects_missing_or_invalid_tokens_as_unauthorized(db: PgPool) {
    let (app, state) = setup(db.clone());
    let user = insert_pengguna(&db, "a@student.ub.ac.id").await;
    insert_mahasiswa(&db, &user, "215150200111001").await;
    let id = user.id_pengguna.to_string();
    let far_future = 4102444800;

    let wrong_key = token(&id, far_future, "another-secret-with-at-least-32-bytes");
    let expired = token(&id, 1000, JWT_SECRET);
    let bad_sub = token("not-a-number", far_future, JWT_SECRET);
    let cases = [
        ("missing", None),
        ("garbage", Some("access_token=not-a-jwt")),
        ("wrong key", Some(wrong_key.as_str())),
        ("expired", Some(expired.as_str())),
        ("bad sub", Some(bad_sub.as_str())),
        ("refresh only", Some("refresh_token=abc")),
    ];

    for (name, cookie) in cases {
        let response = send(app.clone(), "GET", URI, cookie).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{name}");
    }

    // The same pengguna is let through with a valid token, so the cases above fail on the token.
    let cookie = access_cookie(&state, &user);
    let response = send(app, "GET", URI, Some(&cookie)).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[sqlx::test]
async fn only_get_is_routed_and_the_old_route_is_gone(db: PgPool) {
    let (app, state) = setup(db.clone());
    let user = insert_pengguna(&db, "a@student.ub.ac.id").await;
    insert_mahasiswa(&db, &user, "215150200111001").await;

    // The middleware runs before method routing, so a token is needed to learn about the method.
    let without_token = send(app.clone(), "POST", URI, None).await;
    assert_eq!(without_token.status(), StatusCode::UNAUTHORIZED);
    let cookie = access_cookie(&state, &user);
    let with_token = send(app.clone(), "POST", URI, Some(&cookie)).await;
    assert_eq!(with_token.status(), StatusCode::METHOD_NOT_ALLOWED);

    let old = send(app, "GET", "/api/v1/data-diri", None).await;
    assert_eq!(old.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn nim_must_be_unique_but_can_be_empty_for_many(db: PgPool) {
    let first = insert_pengguna(&db, "a@student.ub.ac.id").await;
    let second = insert_pengguna(&db, "b@student.ub.ac.id").await;
    let third = insert_pengguna(&db, "c@student.ub.ac.id").await;
    insert_mahasiswa(&db, &first, "215150200111001").await;

    let duplicate = sqlx::query("INSERT INTO mahasiswa (nim, id_pengguna) VALUES ($1, $2)")
        .bind("215150200111001")
        .bind(second.id_pengguna)
        .execute(&db)
        .await;
    assert!(duplicate.is_err());

    for user in [&second, &third] {
        sqlx::query("INSERT INTO mahasiswa (id_pengguna) VALUES ($1)")
            .bind(user.id_pengguna)
            .execute(&db)
            .await
            .unwrap();
    }
}

#[sqlx::test]
async fn jenis_kelamin_only_accepts_l_or_p(db: PgPool) {
    let user = insert_pengguna(&db, "a@student.ub.ac.id").await;
    sqlx::query("INSERT INTO mahasiswa (id_pengguna) VALUES ($1)")
        .bind(user.id_pengguna)
        .execute(&db)
        .await
        .unwrap();
    let set = |value: &'static str| {
        sqlx::query("UPDATE mahasiswa SET jenis_kelamin = $1")
            .bind(value)
            .execute(&db)
    };

    assert!(set("L").await.is_ok());
    assert!(set("P").await.is_ok());
    assert!(set("X").await.is_err());
}

#[sqlx::test]
async fn pengguna_with_a_mahasiswa_row_cannot_be_deleted(db: PgPool) {
    let user = insert_pengguna(&db, "a@student.ub.ac.id").await;
    insert_mahasiswa(&db, &user, "215150200111001").await;

    let result = sqlx::query("DELETE FROM pengguna WHERE id_pengguna = $1")
        .bind(user.id_pengguna)
        .execute(&db)
        .await;

    assert!(result.is_err());
}

async fn patch(app: Router, cookie: Option<&str>, body: Value) -> Response<Body> {
    let mut request = Request::builder()
        .method("PATCH")
        .uri(URI)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    app.oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

async fn insert_empty_mahasiswa(db: &PgPool, user: &User) {
    sqlx::query("INSERT INTO mahasiswa (id_pengguna) VALUES ($1)")
        .bind(user.id_pengguna)
        .execute(db)
        .await
        .unwrap();
}

#[sqlx::test]
async fn patch_changes_only_the_fields_that_were_sent(db: PgPool) {
    let (app, state) = setup(db.clone());
    let user = insert_pengguna(&db, "budi@student.ub.ac.id").await;
    insert_mahasiswa(&db, &user, "215150200111001").await;

    let cookie = access_cookie(&state, &user);
    let response = patch(app, Some(&cookie), json!({ "no_hp": "+6281234567890" })).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await,
        json!({
            "nim": "215150200111001",
            "email": "budi@student.ub.ac.id",
            "nama": "Budi",
            "program_studi": "Teknik Informatika",
            "ipk": 3.75,
            "no_hp": "+6281234567890",
            "jenis_kelamin": "L",
            "semester": 14,
        })
    );
}

#[sqlx::test]
async fn patch_can_fill_every_column_of_an_empty_row(db: PgPool) {
    let (app, state) = setup(db.clone());
    let user = insert_pengguna(&db, "a@student.ub.ac.id").await;
    insert_empty_mahasiswa(&db, &user).await;

    let cookie = access_cookie(&state, &user);
    let response = patch(
        app.clone(),
        Some(&cookie),
        json!({
            "nim": "215150200111009",
            "nama": "  Ani Wijaya  ",
            "program_studi": "Sistem Informasi",
            "ipk": 3.5,
            "no_hp": "081234567890",
            "jenis_kelamin": "P",
            "semester": 7,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let saved = json_body(send(app, "GET", URI, Some(&cookie)).await).await;
    assert_eq!(
        saved,
        json!({
            "nim": "215150200111009",
            "email": "a@student.ub.ac.id",
            "nama": "Ani Wijaya",
            "program_studi": "Sistem Informasi",
            "ipk": 3.5,
            "no_hp": "081234567890",
            "jenis_kelamin": "P",
            "semester": 7,
        })
    );
}

#[sqlx::test]
async fn patch_with_an_empty_object_changes_nothing(db: PgPool) {
    let (app, state) = setup(db.clone());
    let user = insert_pengguna(&db, "a@student.ub.ac.id").await;
    insert_mahasiswa(&db, &user, "215150200111001").await;

    let cookie = access_cookie(&state, &user);
    let response = patch(app, Some(&cookie), json!({})).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_body(response).await["nim"], "215150200111001");
}

#[sqlx::test]
async fn patch_only_touches_the_token_owner(db: PgPool) {
    let (app, state) = setup(db.clone());
    let budi = insert_pengguna(&db, "budi@student.ub.ac.id").await;
    let ani = insert_pengguna(&db, "ani@student.ub.ac.id").await;
    insert_mahasiswa(&db, &budi, "215150200111001").await;
    insert_mahasiswa(&db, &ani, "215150200111002").await;

    let cookie = access_cookie(&state, &ani);
    patch(app.clone(), Some(&cookie), json!({ "nama": "Ani" })).await;

    let budi_cookie = access_cookie(&state, &budi);
    let response = send(app, "GET", URI, Some(&budi_cookie)).await;
    assert_eq!(json_body(response).await["nama"], "Budi");
}

#[sqlx::test]
async fn patch_rejects_invalid_values_without_saving_anything(db: PgPool) {
    let (app, state) = setup(db.clone());
    let user = insert_pengguna(&db, "a@student.ub.ac.id").await;
    insert_mahasiswa(&db, &user, "215150200111001").await;
    let cookie = access_cookie(&state, &user);

    let cases = [
        json!({ "nim": "" }),
        json!({ "nim": "12a" }),
        json!({ "nim": "1234567890123456" }),
        json!({ "nama": "   " }),
        json!({ "nama": "x".repeat(256) }),
        json!({ "program_studi": "Kedokteran" }),
        json!({ "ipk": 4.01 }),
        json!({ "ipk": -0.01 }),
        json!({ "no_hp": "12345678" }),
        json!({ "no_hp": "1234567890123456" }),
        json!({ "no_hp": "0812-3456-7890" }),
        json!({ "jenis_kelamin": "X" }),
        json!({ "jenis_kelamin": "Laki-laki" }),
        json!({ "semester": 0 }),
        json!({ "semester": 15 }),
        // A valid field next to an invalid one must not be saved either.
        json!({ "nama": "Baru", "semester": 15 }),
    ];

    for body in cases {
        let response = patch(app.clone(), Some(&cookie), body.clone()).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}"
        );
        assert!(json_body(response).await["message"].is_string(), "{body}");
    }

    let saved = json_body(send(app, "GET", URI, Some(&cookie)).await).await;
    assert_eq!(saved["nama"], "Budi");
    assert_eq!(saved["semester"], 14);
}

#[sqlx::test]
async fn patch_rejects_null_wrong_types_and_other_fields(db: PgPool) {
    let (app, state) = setup(db.clone());
    let user = insert_pengguna(&db, "a@student.ub.ac.id").await;
    insert_mahasiswa(&db, &user, "215150200111001").await;
    let cookie = access_cookie(&state, &user);

    let cases = [
        json!({ "nama": null }),
        json!({ "nim": null }),
        json!({ "ipk": "3.5" }),
        json!({ "semester": 1.5 }),
        json!({ "email": "lain@student.ub.ac.id" }),
        json!({ "id_pengguna": 999 }),
        json!({ "nama = 'x', nim": "y" }),
    ];

    for body in cases {
        let response = patch(app.clone(), Some(&cookie), body.clone()).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}"
        );
    }

    let saved = json_body(send(app, "GET", URI, Some(&cookie)).await).await;
    assert_eq!(saved["email"], "a@student.ub.ac.id");
    assert_eq!(saved["nim"], "215150200111001");
}

#[sqlx::test]
async fn patch_stores_values_as_data_not_as_sql(db: PgPool) {
    let (app, state) = setup(db.clone());
    let budi = insert_pengguna(&db, "budi@student.ub.ac.id").await;
    let ani = insert_pengguna(&db, "ani@student.ub.ac.id").await;
    insert_mahasiswa(&db, &budi, "215150200111001").await;
    insert_mahasiswa(&db, &ani, "215150200111002").await;
    let injection = "x', nama = 'hacked' WHERE true; --";

    let cookie = access_cookie(&state, &ani);
    let response = patch(app.clone(), Some(&cookie), json!({ "nama": injection })).await;

    assert_eq!(json_body(response).await["nama"], injection);
    let budi_cookie = access_cookie(&state, &budi);
    let response = send(app, "GET", URI, Some(&budi_cookie)).await;
    assert_eq!(json_body(response).await["nama"], "Budi");
}

#[sqlx::test]
async fn patch_reports_a_nim_used_by_someone_else_as_conflict(db: PgPool) {
    let (app, state) = setup(db.clone());
    let budi = insert_pengguna(&db, "budi@student.ub.ac.id").await;
    let ani = insert_pengguna(&db, "ani@student.ub.ac.id").await;
    insert_mahasiswa(&db, &budi, "215150200111001").await;
    insert_mahasiswa(&db, &ani, "215150200111002").await;

    let cookie = access_cookie(&state, &ani);
    let taken = patch(
        app.clone(),
        Some(&cookie),
        json!({ "nim": "215150200111001" }),
    )
    .await;
    assert_eq!(taken.status(), StatusCode::CONFLICT);
    assert!(json_body(taken).await["message"].is_string());

    let own = patch(app, Some(&cookie), json!({ "nim": "215150200111002" })).await;
    assert_eq!(own.status(), StatusCode::OK);
}

#[sqlx::test]
async fn patch_needs_a_token_and_a_mahasiswa_row(db: PgPool) {
    let (app, state) = setup(db.clone());
    let user = insert_pengguna(&db, "a@student.ub.ac.id").await;

    let without_token = patch(app.clone(), None, json!({ "nama": "x" })).await;
    assert_eq!(without_token.status(), StatusCode::UNAUTHORIZED);

    let cookie = access_cookie(&state, &user);
    let without_row = patch(app, Some(&cookie), json!({ "nama": "x" })).await;
    assert_eq!(without_row.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn browsers_are_allowed_to_send_patch(db: PgPool) {
    let config = Config {
        cors_allowed_origins: vec!["http://frontend.test".parse().unwrap()],
        ..config()
    };
    let app = app::build(&config, AppState::new(&config, db));

    let response = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri(URI)
                .header(header::ORIGIN, "http://frontend.test")
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "PATCH")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let methods = response.headers()[header::ACCESS_CONTROL_ALLOW_METHODS]
        .to_str()
        .unwrap();
    assert!(methods.contains("PATCH"), "{methods}");
}
