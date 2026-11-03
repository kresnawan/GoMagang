use gomagang::{app, config::Config, features::auth, state::AppState};
use sqlx::postgres::PgPoolOptions;

const MAX_DB_CONNECTIONS: u32 = 10;

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

    let app = app::build(&config, AppState::new(&config, db));

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
