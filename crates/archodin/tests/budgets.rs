use std::sync::Arc;

use archodin::graph::{
    AgentNode, ApprovalDecision, Budget, BudgetApprover, Graph, InMemoryCheckpointStore,
    JobOutcome, Pricing, Provider, ScriptedApprover, Usage, UsageMeter,
};
use mockito::Server;
use serde_json::json;

#[derive(Debug, Clone, PartialEq)]
struct Work {
    job: String,
    done: u32,
}

fn work(job: &str) -> Work {
    Work {
        job: job.into(),
        done: 0,
    }
}

/// Three steps: Lumen drafts (60 tokens, $0.25), Qadi reviews (60 tokens, $0.50),
/// Lumen polishes (60 tokens, $0.25).
fn graph(meter: &Arc<UsageMeter>) -> Graph<Work> {
    let mut builder = Graph::builder();
    for (name, agent, cost) in [
        ("draft", "Lumen", 0.25),
        ("review", "Qadi", 0.5),
        ("polish", "Lumen", 0.25),
    ] {
        builder = builder.metered_node(
            name,
            agent,
            meter.clone(),
            |s: &Work| s.job.clone(),
            move |mut s: Work| async move {
                s.done += 1;
                (s, Usage::new(40, 20, cost))
            },
        );
    }
    builder
        .edge("draft", "review")
        .edge("review", "polish")
        .entry("draft")
        .build()
        .unwrap()
}

#[tokio::test]
async fn usage_is_reported_per_agent_and_per_job() {
    let meter = Arc::new(UsageMeter::new());
    let store = InMemoryCheckpointStore::new();
    let approver = BudgetApprover::new(meter.clone(), ScriptedApprover::new([]));
    for job in ["job-1", "job-2"] {
        graph(&meter)
            .run_job(job, work(job), &store, &approver)
            .await
            .unwrap();
    }

    let job = meter.job_usage("job-1");
    assert_eq!(job.total_tokens(), 180);
    assert_eq!(job.prompt_tokens, 120);
    assert_eq!(job.completion_tokens, 60);
    assert!((job.cost - 1.0).abs() < 1e-9);

    // Lumen worked twice in each of the two Jobs.
    let lumen = meter.agent_usage("Lumen");
    assert_eq!(lumen.total_tokens(), 240);
    assert!((lumen.cost - 1.0).abs() < 1e-9);
    assert_eq!(meter.agent_usage("Qadi").total_tokens(), 120);
    assert_eq!(meter.agent_usage("nobody"), Usage::default());
}

#[tokio::test]
async fn a_job_over_budget_pauses_then_continues_when_the_overrun_is_approved() {
    let meter = Arc::new(UsageMeter::new());
    meter.set_job_budget("job-1", Budget::tokens(100));
    let store = InMemoryCheckpointStore::new();

    // After "draft" and "review" the Job has used 120 tokens: "polish" must wait.
    let ask_later = BudgetApprover::new(meter.clone(), ScriptedApprover::new([]));
    let paused = graph(&meter)
        .run_job("job-1", work("job-1"), &store, &ask_later)
        .await
        .unwrap();
    match paused {
        JobOutcome::Paused {
            node,
            prompt,
            state,
        } => {
            assert_eq!(node, "polish");
            assert!(prompt.contains("job-1"), "{prompt}");
            assert_eq!(state.done, 2);
        }
        other => panic!("expected a pause, got {other:?}"),
    }
    let asked = ask_later.inner().requests();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].node, "polish");

    // A person approves the overrun: the Job resumes from its Checkpoint.
    let yes = BudgetApprover::new(
        meter.clone(),
        ScriptedApprover::new([ApprovalDecision::Approved]),
    );
    let done = graph(&meter)
        .run_job("job-1", work("job-1"), &store, &yes)
        .await
        .unwrap();
    assert_eq!(
        done,
        JobOutcome::Finished(Work {
            job: "job-1".into(),
            done: 3
        })
    );
    assert_eq!(meter.job_usage("job-1").total_tokens(), 180);
}

#[tokio::test]
async fn rejecting_the_overrun_stops_the_job() {
    let meter = Arc::new(UsageMeter::new());
    meter.set_job_budget("job-1", Budget::cost(0.5));
    let store = InMemoryCheckpointStore::new();
    let no = BudgetApprover::new(
        meter.clone(),
        ScriptedApprover::new([ApprovalDecision::Rejected]),
    );
    let outcome = graph(&meter)
        .run_job("job-1", work("job-1"), &store, &no)
        .await
        .unwrap();
    assert_eq!(
        outcome,
        JobOutcome::Rejected {
            node: "polish".into(),
            state: Work {
                job: "job-1".into(),
                done: 2
            }
        }
    );
    assert_eq!(meter.job_usage("job-1").total_tokens(), 120);
}

#[tokio::test]
async fn within_budget_the_person_is_never_asked() {
    let meter = Arc::new(UsageMeter::new());
    meter.set_job_budget("job-1", Budget::tokens(1000).with_cost(10.0));
    let store = InMemoryCheckpointStore::new();
    let approver = BudgetApprover::new(meter.clone(), ScriptedApprover::new([]));
    let outcome = graph(&meter)
        .run_job("job-1", work("job-1"), &store, &approver)
        .await
        .unwrap();
    assert!(matches!(outcome, JobOutcome::Finished(_)));
    assert!(approver.inner().requests().is_empty());
}

#[tokio::test]
async fn an_agent_budget_works_the_same_way() {
    let meter = Arc::new(UsageMeter::new());
    // Lumen may spend $0.20 and its first step costs $0.25; only Lumen's own
    // steps are gated, so "review" (Qadi) runs and "polish" (Lumen) waits.
    meter.set_agent_budget("Lumen", Budget::cost(0.2));
    let store = InMemoryCheckpointStore::new();
    let wait = BudgetApprover::new(meter.clone(), ScriptedApprover::new([]));
    let paused = graph(&meter)
        .run_job("job-1", work("job-1"), &store, &wait)
        .await
        .unwrap();
    assert!(matches!(paused, JobOutcome::Paused { ref node, .. } if node == "polish"));
    if let JobOutcome::Paused { prompt, .. } = paused {
        assert!(prompt.contains("Lumen"), "{prompt}");
    }

    let yes = BudgetApprover::new(
        meter.clone(),
        ScriptedApprover::new([ApprovalDecision::Approved]),
    );
    let done = graph(&meter)
        .run_job("job-1", work("job-1"), &store, &yes)
        .await
        .unwrap();
    assert!(matches!(done, JobOutcome::Finished(_)));

    // A rejection stops a Job the same way.
    let meter = Arc::new(UsageMeter::new());
    meter.set_agent_budget("Lumen", Budget::cost(0.2));
    let no = BudgetApprover::new(
        meter.clone(),
        ScriptedApprover::new([ApprovalDecision::Rejected]),
    );
    let outcome = graph(&meter)
        .run_job("job-9", work("job-9"), &InMemoryCheckpointStore::new(), &no)
        .await
        .unwrap();
    assert!(matches!(outcome, JobOutcome::Rejected { ref node, .. } if node == "polish"));
}

#[tokio::test]
async fn an_agent_node_reports_the_tokens_the_provider_used() {
    let mut server = Server::new_async().await;
    server
        .mock("POST", "/chat/completions")
        .with_header("content-type", "application/json")
        .with_body(
            json!({
                "id": "c1", "object": "chat.completion", "created": 0, "model": "fake",
                "choices": [{"index": 0, "message": {"role": "assistant", "content": "hi"},
                    "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 1000, "completion_tokens": 500, "total_tokens": 1500}
            })
            .to_string(),
        )
        .create_async()
        .await;
    let provider = Provider::openai_compatible(server.url(), "key", "any-model");
    let agent = AgentNode::new(provider, "Be brief");
    let (answer, usage) = agent.answer_with_usage("hello").await.unwrap();
    assert_eq!(answer, "hi");
    assert_eq!(usage.prompt_tokens, 1000);
    assert_eq!(usage.completion_tokens, 500);

    // $2 per million prompt tokens and $4 per million completion tokens.
    let priced = Pricing::per_million(2.0, 4.0).price(usage);
    assert!((priced.cost - 0.004).abs() < 1e-9, "{}", priced.cost);
}
