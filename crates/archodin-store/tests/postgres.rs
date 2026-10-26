//! The behaviour suite against Postgres in a testcontainers container. Needs
//! Docker, so every test is ignored by default: run with `--ignored`.
#![cfg(feature = "postgres")]

#[macro_use]
mod common;

use std::sync::Arc;

use archodin_store::SqlStore;
use common::opener;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

type Started = (ContainerAsync<GenericImage>, String);

async fn start() -> Started {
    let container = GenericImage::new("postgres", "16")
        .with_exposed_port(5432.tcp())
        .with_wait_for(WaitFor::message_on_stderr(
            "database system is ready to accept connections",
        ))
        .with_env_var("POSTGRES_PASSWORD", "archodin")
        .start()
        .await
        .expect("start postgres (needs Docker)");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:archodin@127.0.0.1:{port}/postgres");
    (container, url)
}

suite!(
    {
        // One container per test, started on first use and kept for every "restart".
        let started: Arc<tokio::sync::OnceCell<Started>> = Arc::default();
        opener(move || {
            let started = started.clone();
            async move {
                let (_container, url) = started.get_or_init(start).await;
                // The image logs the ready message once for its temporary server, so retry.
                let mut tries = 0;
                loop {
                    match SqlStore::connect(url).await {
                        Ok(store) => break Arc::new(store),
                        Err(_) if tries < 30 => {
                            tries += 1;
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        }
                        Err(e) => panic!("cannot reach postgres: {e}"),
                    }
                }
            }
        })
    },
    [ignore = "needs Docker for testcontainers"]
);
