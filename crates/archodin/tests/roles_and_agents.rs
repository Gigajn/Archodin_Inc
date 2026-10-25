use archodin::graph::{branch_name, Graph, Provider, Role, Roster, RosterError};
use mockito::{Matcher, Server, ServerGuard};
use serde_json::json;

fn completion(content: &str) -> String {
    json!({
        "id": "c1", "object": "chat.completion", "created": 0, "model": "fake",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": content},
            "finish_reason": "stop"}]
    })
    .to_string()
}

/// A fake Provider that answers `answer` to any request whose body matches `body`.
async fn fake(body: &str, answer: &str) -> ServerGuard {
    let mut server = Server::new_async().await;
    server
        .mock("POST", "/chat/completions")
        .match_body(Matcher::Regex(body.into()))
        .with_header("content-type", "application/json")
        .with_body(completion(answer))
        .create_async()
        .await;
    server
}

fn provider(server: &ServerGuard) -> Provider {
    Provider::openai_compatible(server.url(), "key", "any-model")
}

#[tokio::test]
async fn several_agents_share_one_role_with_unique_names() {
    let roster = Roster::new();
    roster.add_role(Role::new("Engineer", "Write code"));
    let ada = roster.hire("Engineer", "Ada").unwrap();
    let bo = roster.hire("Engineer", "Bo").unwrap();
    assert_eq!(ada.name(), "Ada");
    assert_eq!(bo.role(), "Engineer");
    assert_eq!(ada.title(), "Ada (Engineer)");
    assert_eq!(
        roster.hire("Engineer", "Ada").err(),
        Some(RosterError::NameTaken("Ada".into()))
    );
    assert_eq!(
        roster.hire("Designer", "Cy").err(),
        Some(RosterError::UnknownRole("Designer".into()))
    );
    assert!(matches!(
        roster.hire("Engineer", "bad/name"),
        Err(RosterError::InvalidName(_))
    ));
    let names: Vec<_> = roster
        .agents()
        .iter()
        .map(|a| a.name().to_string())
        .collect();
    assert_eq!(names, ["Ada", "Bo"]);
    assert_eq!(roster.agent("Bo").unwrap().role(), "Engineer");
}

#[tokio::test]
async fn an_agent_keeps_its_conversation_between_two_jobs() {
    let first = fake("my name is Kudo", "Noted, Kudo").await;
    let roster = Roster::new();
    roster.add_role(Role::new("Engineer", "Be brief").model_chain(vec![provider(&first)]));
    let ada = roster.hire("Engineer", "Ada").unwrap();
    let bo = roster.hire("Engineer", "Bo").unwrap();
    assert_eq!(ada.ask("my name is Kudo").await.unwrap(), "Noted, Kudo");

    // The second Job reaches the Provider with the first Job's turn in it.
    let mut second = Server::new_async().await;
    second
        .mock("POST", "/chat/completions")
        .match_body(Matcher::AllOf(vec![
            Matcher::Regex("my name is Kudo".into()),
            Matcher::Regex("Noted, Kudo".into()),
            Matcher::Regex("who am I".into()),
        ]))
        .with_header("content-type", "application/json")
        .with_body(completion("Kudo"))
        .create_async()
        .await;
    ada.set_model_chain(vec![provider(&second)]);
    assert_eq!(ada.ask("who am I").await.unwrap(), "Kudo");
    assert_eq!(ada.history_len(), 4);

    // A colleague of the same Role does not share Ada's conversation.
    assert_eq!(bo.history_len(), 0);
}

#[tokio::test]
async fn a_per_agent_model_chain_overrides_the_roles() {
    let role_server = fake("Be brief", "from the role chain").await;
    let own_server = fake("Be brief", "from the agent chain").await;
    let roster = Roster::new();
    roster.add_role(Role::new("Engineer", "Be brief").model_chain(vec![provider(&role_server)]));
    let ada = roster.hire("Engineer", "Ada").unwrap();
    let bo = roster.hire("Engineer", "Bo").unwrap();
    bo.set_model_chain(vec![provider(&own_server)]);
    assert_eq!(ada.ask("hi").await.unwrap(), "from the role chain");
    assert_eq!(bo.ask("hi").await.unwrap(), "from the agent chain");
    bo.clear_model_chain();
    assert_eq!(bo.ask("again").await.unwrap(), "from the role chain");
}

#[test]
fn branch_names_start_with_the_agent_name() {
    assert_eq!(branch_name("Ada", "fix login bug"), "Ada/fix-login-bug");
    assert_eq!(branch_name("Ada", "  Add  README! "), "Ada/add-readme");
}

#[tokio::test]
async fn a_named_agent_runs_as_a_graph_node() {
    let server = fake("Be brief", "done").await;
    let roster = Roster::new();
    roster.add_role(Role::new("Engineer", "Be brief").model_chain(vec![provider(&server)]));
    let ada = roster.hire("Engineer", "Ada").unwrap();
    let graph = Graph::builder()
        .named_agent_node(
            "ada",
            ada,
            |s: &(String, String)| s.0.clone(),
            |mut s: (String, String), a| {
                s.1 = a.unwrap();
                s
            },
        )
        .entry("ada")
        .build()
        .unwrap();
    let out = graph.run(("go".into(), String::new())).await.unwrap();
    assert_eq!(out.1, "done");
}
