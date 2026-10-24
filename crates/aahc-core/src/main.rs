use aahc_core::app::app;
use aahc_core::config::Config;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_owned()))
        .init();

    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            tracing::error!("cannot start the office: {error}");
            eprintln!("cannot start the office: {error}");
            std::process::exit(2);
        }
    };
    tracing::info!(db = %config.db_path.display(), "database configured");

    let listener = match tokio::net::TcpListener::bind(config.bind_addr).await {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!(addr = %config.bind_addr, "cannot bind: {error}");
            eprintln!("cannot bind {}: {error}", config.bind_addr);
            std::process::exit(1);
        }
    };

    tracing::info!(addr = %config.bind_addr, "aahc-core listening");
    if let Err(error) = axum::serve(listener, app()).await {
        tracing::error!("server stopped: {error}");
        std::process::exit(1);
    }
}
