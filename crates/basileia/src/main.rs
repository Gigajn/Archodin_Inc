//! Basileia server binary: read config, assemble the company, serve it.
//!
//! Usage: `cargo run -p basileia` (see the crate README for configuration).
//! The server refuses to start without an admin password (KR-07).

use std::net::SocketAddr;

use basileia::{assemble, Config, Error};

/// Validates the arguments of the `generate-vault-key` subcommand: it
/// takes no further arguments. Anything after it is a usage error — the
/// caller prints the message to stderr and exits non-zero, instead of
/// silently ignoring the extra arguments.
fn check_generate_vault_key_args(argv: &[String]) -> Result<(), String> {
    if argv.get(1).map(String::as_str) != Some("generate-vault-key") {
        return Ok(());
    }
    if argv.len() > 2 {
        return Err(format!(
            "usage: basileia generate-vault-key (takes no arguments); got: {}",
            argv[2..].join(" ")
        ));
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    // `basileia generate-vault-key` prints base64 of 32 random bytes (ADR
    // 0021) and exits, WITHOUT reading `Config` — no admin password is
    // needed to mint a Vault key. The operator stores the output in
    // `BASILEIA_VAULT_KEY` or a 0600 file read via
    // `BASILEIA_VAULT_KEY_FILE`.
    if std::env::args().nth(1).as_deref() == Some("generate-vault-key") {
        let argv: Vec<String> = std::env::args().collect();
        if let Err(usage) = check_generate_vault_key_args(&argv) {
            eprintln!("basileia: {usage}");
            std::process::exit(2);
        }
        let mut key = zeroize::Zeroizing::new([0u8; 32]);
        if let Err(error) = getrandom::fill(&mut *key) {
            eprintln!("basileia: cannot generate a vault key: {error}");
            std::process::exit(1);
        }
        use base64::Engine as _;
        println!("{}", base64::engine::general_purpose::STANDARD.encode(*key));
        return;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn generate_vault_key_rejects_extra_arguments() {
        assert!(check_generate_vault_key_args(&argv(&["basileia", "generate-vault-key"])).is_ok());
        let err =
            check_generate_vault_key_args(&argv(&["basileia", "generate-vault-key", "--force"]))
                .unwrap_err();
        assert!(err.contains("usage: basileia generate-vault-key"), "{err}");
        assert!(err.contains("--force"), "{err}");
        let err = check_generate_vault_key_args(&argv(&[
            "basileia",
            "generate-vault-key",
            "extra",
            "args",
        ]))
        .unwrap_err();
        assert!(err.contains("usage: basileia generate-vault-key"), "{err}");
        // No subcommand, or another subcommand, is untouched.
        assert!(check_generate_vault_key_args(&argv(&["basileia"])).is_ok());
        assert!(
            check_generate_vault_key_args(&argv(&["basileia", "serve", "--port", "8080"])).is_ok()
        );
    }
}
