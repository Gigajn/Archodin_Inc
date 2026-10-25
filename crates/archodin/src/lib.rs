//! Archodin: the umbrella crate for the AI-staffed company library.
//!
//! Re-exports langchain-rust (kept untouched, see ADR 0001) and the Archodin crates.

pub use archodin_graph as graph;
pub use langchain_rust;

/// The version of the Archodin library.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
