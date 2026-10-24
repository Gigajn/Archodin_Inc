//! Seam: configuration loaded from the environment. Behaviour only.

use std::collections::HashMap;

use aahc_core::config::{Config, ConfigError};

fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    move |name: &str| map.get(name).cloned()
}

#[test]
fn missing_database_path_is_a_typed_error_naming_the_variable() {
    let error = Config::from_lookup(lookup(&[])).expect_err("must reject a missing database path");

    match &error {
        ConfigError::Missing { name } => assert_eq!(name, "AAHC_DB_PATH"),
        other => panic!("expected a Missing error, got {other:?}"),
    }
    assert!(
        error.to_string().contains("AAHC_DB_PATH"),
        "the message must name the variable, got: {error}"
    );
}

#[test]
fn database_path_is_read_from_the_environment() {
    let config = Config::from_lookup(lookup(&[("AAHC_DB_PATH", "/tmp/aahc/office.db")]))
        .expect("valid config");

    assert_eq!(config.db_path.to_string_lossy(), "/tmp/aahc/office.db");
}

#[test]
fn bind_address_defaults_to_localhost() {
    let config =
        Config::from_lookup(lookup(&[("AAHC_DB_PATH", "/tmp/office.db")])).expect("valid config");

    assert_eq!(
        config.bind_addr.to_string(),
        "127.0.0.1:8000",
        "the office must not be exposed to the network by default"
    );
}

#[test]
fn bind_address_is_configurable() {
    let config = Config::from_lookup(lookup(&[
        ("AAHC_DB_PATH", "/tmp/office.db"),
        ("AAHC_BIND_ADDR", "0.0.0.0:9000"),
    ]))
    .expect("valid config");

    assert_eq!(config.bind_addr.to_string(), "0.0.0.0:9000");
}

#[test]
fn invalid_bind_address_is_a_typed_error_naming_the_variable() {
    let error = Config::from_lookup(lookup(&[
        ("AAHC_DB_PATH", "/tmp/office.db"),
        ("AAHC_BIND_ADDR", "not-an-address"),
    ]))
    .expect_err("must reject an unparsable address");

    match &error {
        ConfigError::Invalid { name, .. } => assert_eq!(name, "AAHC_BIND_ADDR"),
        other => panic!("expected an Invalid error, got {other:?}"),
    }
}
