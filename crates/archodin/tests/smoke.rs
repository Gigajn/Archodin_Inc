#[test]
fn exposes_version_and_reexports_langchain_rust() {
    assert!(!archodin::version().is_empty());
    // The umbrella crate re-exports langchain-rust, so downstream code needs one dependency.
    let args: archodin::langchain_rust::prompt::PromptArgs = Default::default();
    assert!(args.is_empty());
}
