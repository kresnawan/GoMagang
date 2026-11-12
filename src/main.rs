use gomagang::{
    app,
    config::Config,
    features::{auth, pendaftaran},
    state::AppState,
};
use sqlx::postgres::PgPoolOptions;

const MAX_DB_CONNECTIONS: u32 = 10;

/// Titik masuk server backend GoMagang.
///
/// Urutan yang dijalankan saat start:
/// 1. Membaca file `.env` bila ada. File ini opsional, sehingga di production variabel environment
///    bisa di-set langsung. File yang ada tetapi formatnya rusak membuat server berhenti.
/// 2. Membaca seluruh konfigurasi lewat [`Config::from_env`].
/// 3. Membuka pool koneksi PostgreSQL (maksimal [`MAX_DB_CONNECTIONS`] koneksi) dan menjalankan
///    migration yang tertanam di binary.
/// 4. Menyalakan proses latar belakang: pembersihan refresh token kedaluwarsa dan pembatalan
///    otomatis
///    pendaftaran `Diterima` yang tidak dikonfirmasi dalam 12 jam.
/// 5. Menyusun router dan melayani HTTP di `0.0.0.0:3000`.
///
/// Kegagalan pada langkah mana pun membuat proses panic, karena server tidak bisa berjalan dengan
/// benar tanpa konfigurasi, database, atau port yang valid.
#[tokio::main]
async fn main() {
    // The .env file is optional, e.g. production can set real environment variables instead.
    if let Err(err) = dotenvy::dotenv()
        && !err.not_found()
    {
        panic!("failed to load .env: {err}");
    }

    let config = Config::from_env();

    let db = PgPoolOptions::new()
        .max_connections(MAX_DB_CONNECTIONS)
        .connect(&config.database_url)
        .await
        .expect("failed to connect to the database");
    sqlx::migrate!()
        .run(&db)
        .await
        .expect("failed to run database migrations");
    auth::spawn_token_cleanup(db.clone());

    let state = AppState::new(&config, db);
    pendaftaran::spawn_pembatalan_otomatis(state.clone());

    let app = app::build(&config, state);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
