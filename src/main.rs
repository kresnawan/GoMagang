use gomagang::{app, config::Config, state::AppState};

#[tokio::main]
async fn main() {
    // The .env file is optional, e.g. production can set real environment variables instead.
    if let Err(err) = dotenvy::dotenv()
        && !err.not_found()
    {
        panic!("failed to load .env: {err}");
    }

    let config = Config::from_env();
    let app = app::build(&config, AppState::new(&config));

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
