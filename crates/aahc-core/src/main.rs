use aahc_core::app::app;
use aahc_core::config::Config;
use aahc_core::db;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_owned()))
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("init-db") {
        let db_path = args
            .iter()
            .position(|arg| arg == "--db")
            .and_then(|index| args.get(index + 1))
            .map(std::path::PathBuf::from);
        let db_path = match db_path {
            Some(path) => path,
            None => match Config::from_env() {
                Ok(config) => config.db_path,
                Err(error) => {
                    eprintln!("cannot resolve database path: {error}");
                    std::process::exit(2);
                }
            },
        };
        match db::init_database(&db_path).await {
            Ok(summary) => {
                println!(
                    "AAHC database ready at {}: {} tables, {}/{} lanes ({} new)",
                    db_path.display(),
                    summary.tables,
                    summary.lanes_total,
                    summary.lanes_total,
                    summary.lanes_inserted,
                );
            }
            Err(error) => {
                eprintln!("cannot initialise database: {error}");
                std::process::exit(1);
            }
        }
        return;
    }

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
