//! Basileia server binary: read config, assemble the company, serve it.
//!
//! Usage: `cargo run -p basileia` (see the crate README for configuration).
//! The server refuses to start without an admin password (KR-07).

use std::net::SocketAddr;

use basileia::{assemble, Config, Error};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("basileia: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Error> {
    let config = Config::from_env()?;
    let bind = config.bind;
    let app = assemble(config).await?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    eprintln!("basileia: listening on http://{bind} — login required");
    eprintln!("basileia: database and checkpoints survive restarts; jobs resume by id");
    // `ConnectInfo` carries the peer address into handlers: on the KR-07
    // branch the login lockout counts failures per (client IP, username),
    // and without this the IP is always "unknown" — so one attacker's five
    // bad guesses could lock the admin account for everyone (lockout-DoS).
    // On `main` no handler reads `ConnectInfo` yet, so this is inert there.
    axum::serve(
        listener,
        app.router
            .into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
