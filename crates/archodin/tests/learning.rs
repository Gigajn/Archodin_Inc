//! Learning from experience and the Akasha (ticket 18, ADR 0004).

use archodin::graph::{
    Agent, Akasha, ApprovalDecision, Learner, Provider, Role, Roster, ScriptedApprover, Skill,
    SkillLibrary, TypedTool,
};
use mockito::{Matcher, Mock, Server, ServerGuard};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, JsonSchema)]
struct Empty {}

#[derive(Debug, thiserror::Error)]
#[error("never")]
struct Never;

struct Probe;

impl TypedTool for Probe {
    type Input = Empty;
    type Error = Never;
    fn name(&self) -> String {
        "probe".into()
    }
    fn description(&self) -> String {
        "Probes".into()
    }
    async fn call(&self, _: Empty) -> Result<String, Never> {
        Ok("probed".into())
    }
}

fn completion(content: Option<&str>, calls: Vec<serde_json::Value>) -> String {
    let mut message = json!({"role": "assistant", "content": content});
    if !calls.is_empty() {
        message["tool_calls"] = json!(calls);
    }
    json!({
        "id": "c1", "object": "chat.completion", "created": 0, "model": "fake",
        "choices": [{"index": 0, "message": message,
            "finish_reason": if calls.is_empty() { "stop" } else { "tool_calls" }}]
    })
    .to_string()
}

fn probes(n: usize) -> Vec<serde_json::Value> {
    (0..n)
        .map(|i| {
            json!({"id": format!("t{i}"), "type": "function",
                "function": {"name": "probe", "arguments": "{}"}})
        })
        .collect()
}

/// An agent server: asks for `n` probe calls, then answers "done".
async fn worker(n: usize) -> ServerGuard {
    let mut server = Server::new_async().await;
    server
        .mock("POST", "/chat/completions")
        .match_body(Matcher::Regex(r#""role":\s*"tool""#.into()))
        .with_header("content-type", "application/json")
        .with_body(completion(Some("done"), vec![]))
        .create_async()
        .await;
    server
        .mock("POST", "/chat/completions")
        .match_body(Matcher::Regex(r#""name":\s*"probe""#.into()))
        .with_header("content-type", "application/json")
        .with_body(completion(None, probes(n)))
        .create_async()
        .await;
    server
}

/// A server that always replies `reply` and must be hit exactly `hits` times.
async fn writer(reply: &str, hits: usize) -> (ServerGuard, Mock) {
    let mut server = Server::new_async().await;
    let mock = server
        .mock("POST", "/chat/completions")
        .with_header("content-type", "application/json")
        .with_body(completion(Some(reply), vec![]))
        .expect(hits)
        .create_async()
        .await;
    (server, mock)
}

fn provider(server: &ServerGuard) -> Provider {
    Provider::openai_compatible(server.url(), "key", "any-model")
}

const SKILL_MD: &str = "---\nname: probe-first\ndescription: Probe before acting\nkind: discipline\n---\nProbe five times.\n";
const REFINED_MD: &str = "---\nname: probe-first\ndescription: Probe before acting\nkind: discipline\n---\nProbe five times, then confirm with the person.\n";

fn setup(server: &ServerGuard, learner: &Learner) -> Agent {
    let roster = Roster::new();
    roster.add_role(
        Role::new("Engineer", "Be brief")
            .tool(learner.counting(Probe))
            .model_chain(vec![provider(server)]),
    );
    roster.hire("Engineer", "Ada").unwrap()
}

async fn approve_all(library: &SkillLibrary) {
    library
        .review_pending(&ScriptedApprover::new(vec![ApprovalDecision::Approved]))
        .await;
}

#[tokio::test]
async fn run_with_five_tool_calls_proposes_a_skill_for_approval() {
    let agent_server = worker(5).await;
    let (writer_server, hits) = writer(SKILL_MD, 1).await;
    let library = SkillLibrary::new();
    let learner = Learner::new(library.clone(), provider(&writer_server));
    let ada = setup(&agent_server, &learner);

    let outcome = learner.ask(&ada, "Check the service").await.unwrap();
    assert_eq!(outcome.answer, "done");
    assert_eq!(outcome.tool_calls, 5);
    assert_eq!(outcome.proposed.as_deref(), Some("probe-first"));
    hits.assert_async().await;

    let pending = library.pending();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].role, "Engineer");
    approve_all(&library).await;
    assert_eq!(library.skills_for_agent(&ada)[0].name(), "probe-first");
}

#[tokio::test]
async fn run_with_fewer_tool_calls_proposes_nothing() {
    let agent_server = worker(4).await;
    let (writer_server, hits) = writer(SKILL_MD, 0).await;
    let library = SkillLibrary::new();
    let learner = Learner::new(library.clone(), provider(&writer_server));
    let ada = setup(&agent_server, &learner);

    let outcome = learner.ask(&ada, "Check the service").await.unwrap();
    assert_eq!(outcome.tool_calls, 4);
    assert_eq!(outcome.proposed, None);
    hits.assert_async().await;
    assert!(library.pending().is_empty());
}

#[tokio::test]
async fn correction_proposes_a_refinement_of_the_role_skill() {
    let mut plain = Server::new_async().await;
    plain
        .mock("POST", "/chat/completions")
        .with_header("content-type", "application/json")
        .with_body(completion(Some("noted"), vec![]))
        .create_async()
        .await;
    let fenced = format!("```markdown\n{REFINED_MD}```");
    let mut writer_server = Server::new_async().await;
    let hits = writer_server
        .mock("POST", "/chat/completions")
        .match_body(Matcher::Regex("always confirm first".into()))
        .match_body(Matcher::Regex("Probe five times".into()))
        .with_header("content-type", "application/json")
        .with_body(completion(Some(&fenced), vec![]))
        .expect(1)
        .create_async()
        .await;

    let library = SkillLibrary::new();
    library.propose("Engineer", Skill::from_markdown(SKILL_MD).unwrap());
    approve_all(&library).await;
    let learner = Learner::new(library.clone(), provider(&writer_server));
    let roster = Roster::new();
    roster.add_role(Role::new("Engineer", "Be brief").model_chain(vec![provider(&plain)]));
    let ada = roster.hire("Engineer", "Ada").unwrap();

    let outcome = learner.correct(&ada, "always confirm first").await.unwrap();
    assert_eq!(outcome.proposed.as_deref(), Some("probe-first"));
    hits.assert_async().await;
    let pending = library.pending();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].skill.body().contains("confirm with the person"));
    approve_all(&library).await;
    assert_eq!(library.versions("Engineer", "probe-first"), vec![1, 2]);
}

#[tokio::test]
async fn an_unreadable_skill_from_the_writer_is_an_error_not_a_proposal() {
    let agent_server = worker(5).await;
    let (writer_server, _) = writer("not a skill", 1).await;
    let library = SkillLibrary::new();
    let learner = Learner::new(library.clone(), provider(&writer_server));
    let ada = setup(&agent_server, &learner);
    assert!(learner.ask(&ada, "Check").await.is_err());
    assert!(library.pending().is_empty());
}

#[test]
fn facts_learned_by_one_agent_are_readable_by_every_agent_of_the_role() {
    let akasha = Akasha::new();
    let roster = Roster::new();
    roster.add_role(Role::new("Engineer", "x"));
    roster.add_role(Role::new("Designer", "x"));
    let ada = roster.hire("Engineer", "Ada").unwrap();
    let bo = roster.hire("Engineer", "Bo").unwrap();
    let cy = roster.hire("Designer", "Cy").unwrap();
    assert!(akasha.remember(&ada, "Acme pays on the 5th"));
    assert_eq!(akasha.facts_for_agent(&bo), vec!["Acme pays on the 5th"]);
    assert!(akasha.facts_for_agent(&cy).is_empty());
}

#[test]
fn duplicate_fact_does_not_grow_the_akasha() {
    let akasha = Akasha::new();
    assert!(akasha.remember_for_role("Engineer", "Acme pays on the 5th"));
    assert!(!akasha.remember_for_role("Engineer", "  acme  pays on the 5th. "));
    assert!(!akasha.remember_for_role("Engineer", "Acme pays on the 5th"));
    assert_eq!(akasha.facts("Engineer").len(), 1);
    assert!(akasha.remember_for_role("Designer", "Acme pays on the 5th"));
    assert!(akasha
        .briefing("Engineer")
        .contains("- Acme pays on the 5th"));
}

#[tokio::test]
async fn an_agent_remembers_a_fact_through_its_tool() {
    let akasha = Akasha::new();
    let mut server = Server::new_async().await;
    server
        .mock("POST", "/chat/completions")
        .match_body(Matcher::Regex(r#""role":\s*"tool""#.into()))
        .with_header("content-type", "application/json")
        .with_body(completion(Some("ok"), vec![]))
        .create_async()
        .await;
    let call = json!({"id": "t1", "type": "function", "function":
        {"name": "remember", "arguments": r#"{"fact":"Client Acme uses Postgres"}"#}});
    server
        .mock("POST", "/chat/completions")
        .match_body(Matcher::Regex(r#""name":\s*"remember""#.into()))
        .with_header("content-type", "application/json")
        .with_body(completion(None, vec![call]))
        .create_async()
        .await;
    let roster = Roster::new();
    roster.add_role(
        Role::new("Engineer", "x")
            .tool(akasha.remember_tool("Engineer"))
            .model_chain(vec![provider(&server)]),
    );
    let ada = roster.hire("Engineer", "Ada").unwrap();
    ada.ask("Note that Acme uses Postgres").await.unwrap();
    assert_eq!(akasha.facts("Engineer"), vec!["Client Acme uses Postgres"]);
}
