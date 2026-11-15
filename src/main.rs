use gomagang::{
    app,
    config::Config,
    features::{auth, pendaftaran},
    logging,
    state::AppState,
};
use sqlx::postgres::PgPoolOptions;

const MAX_DB_CONNECTIONS: u32 = 10;

/// Titik masuk server backend GoMagang.
///
/// Urutan yang dijalankan saat start:
/// 1. Membaca file `.env` bila ada. File ini opsional, sehingga di production variabel environment
///    bisa di-set langsung. File yang ada tetapi formatnya rusak membuat server berhenti.
/// 2. Memasang logger JSON ke stdout lewat [`logging::init`], sehingga `RUST_LOG` bisa di-set di
///    `.env`. Panic sesudah langkah ini juga tercatat sebagai log JSON.
/// 3. Membaca seluruh konfigurasi lewat [`Config::from_env`].
/// 4. Membuka pool koneksi PostgreSQL (maksimal [`MAX_DB_CONNECTIONS`] koneksi) dan menjalankan
///    migration yang tertanam di binary.
/// 5. Menyalakan proses latar belakang: pembersihan refresh token kedaluwarsa dan pembatalan
///    otomatis pendaftaran `Diterima` yang tidak dikonfirmasi dalam 12 jam.
/// 6. Menyusun router dan melayani HTTP di `0.0.0.0:3000`.
/// 7. Saat menerima SIGTERM (misalnya dari `docker stop`) atau Ctrl+C, server berhenti menerima
///    koneksi baru, menunggu request yang sedang berjalan selesai, lalu menutup pool database dan
///    keluar (lihat [`shutdown_signal`]).
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

    logging::init();

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

    let state = AppState::new(&config, db.clone());
    pendaftaran::spawn_pembatalan_otomatis(state.clone());

    let app = app::build(&config, state);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        address = %listener.local_addr().unwrap(),
        "server started"
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    db.close().await;
    tracing::info!("server stopped");
}

/// Selesai saat proses diminta berhenti: Ctrl+C (SIGINT) atau, di Unix, SIGTERM.
///
/// SIGTERM adalah sinyal yang dikirim `docker stop`. Tanpa handler ini, binary yang berjalan
/// sebagai PID 1 di container mengabaikannya, sehingga Docker menunggu lalu mematikan proses
/// secara paksa dan request yang sedang berjalan terputus. Tidak ada batas waktu di sini; batasnya
/// mengikuti Docker (`stop_grace_period`, default 10 detik).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for Ctrl+C");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to listen for SIGTERM")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }

    tracing::info!("shutdown signal received, finishing in-flight requests");
}
