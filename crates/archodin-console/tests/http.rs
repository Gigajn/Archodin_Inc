//! The Agora through its HTTP API, driven in-process (no network ports).

use std::sync::Arc;
use std::time::Duration;

use archodin_console::{pump, Agora, JobContext};
use archodin_graph::{
    Budget, BudgetApprover, Graph, InMemoryCheckpointStore, McpLibrary, McpServer, Skill,
    SkillLibrary, Usage, UsageMeter,
};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

async fn call(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, String) {
    let mut req = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(v) => {
            req = req.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let res = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn json_of(app: &Router, method: &str, uri: &str, body: Option<Value>) -> Value {
    let (status, text) = call(app, method, uri, body).await;
    assert!(status.is_success(), "{method} {uri} -> {status}: {text}");
    serde_json::from_str(&text).unwrap()
}

async fn wait_for_status(app: &Router, id: u64, want: &str) {
    for _ in 0..200 {
        let job = json_of(app, "GET", &format!("/api/jobs/{id}"), None).await;
        if job["status"] == want {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("job {id} never reached status {want}");
}

/// A runner for a Graph with one plain Node, which echoes the request.
fn echo_runner() -> impl Fn(JobContext) -> futures::future::BoxFuture<'static, ()> + Send + Sync {
    let graph = Arc::new(
        Graph::<String>::builder()
            .node("echo", |s: String| async move { format!("done: {s}") })
            .entry("echo")
            .build()
            .unwrap(),
    );
    let store = Arc::new(InMemoryCheckpointStore::<String>::new());
    move |ctx: JobContext| {
        let (graph, store) = (graph.clone(), store.clone());
        Box::pin(async move {
            let events =
                graph.stream_job(&ctx.job, ctx.request.request.clone(), &*store, &ctx.agora);
            pump(events, &ctx.sink).await;
        })
    }
}

#[tokio::test]
async fn a_submitted_job_runs_and_its_events_stream() {
    let agora = Agora::builder().runner(echo_runner()).build();
    let app = agora.router();

    let created = json_of(
        &app,
        "POST",
        "/api/jobs",
        Some(json!({
            "request": "write the report",
            "role": "writer",
            "links": ["https://example.com/brief"],
            "github_url": "https://github.com/Gigajn/Archodin_Inc/issues/3",
            "files": [{"name": "notes.txt", "content": "hello"}]
        })),
    )
    .await;
    let id = created["id"].as_u64().unwrap();
    wait_for_status(&app, id, "finished").await;

    let (status, sse) = call(&app, "GET", &format!("/api/jobs/{id}/events"), None).await;
    assert_eq!(status, StatusCode::OK);
    let started = sse.find("event: job_started").expect("job_started");
    let node = sse.find("event: node_started").expect("node_started");
    let finished = sse.find("event: job_finished").expect("job_finished");
    assert!(started < node && node < finished);
    assert!(sse.contains("done: write the report"));

    let job = json_of(&app, "GET", &format!("/api/jobs/{id}"), None).await;
    assert_eq!(job["role"], "writer");
    assert_eq!(job["links"][0], "https://example.com/brief");
    let list = json_of(&app, "GET", "/api/jobs", None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn bad_submissions_are_refused() {
    let app = Agora::builder().runner(echo_runner()).build().router();
    let (s, _) = call(&app, "POST", "/api/jobs", Some(json!({"request": "  "}))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(
        &app,
        "POST",
        "/api/jobs",
        Some(json!({"request": "x", "github_url": "https://evil.example/x"})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(
        &app,
        "POST",
        "/api/jobs",
        Some(json!({"request": "x", "links": ["javascript:alert(1)"]})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(&app, "GET", "/api/jobs/99", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_running_job_can_be_cancelled() {
    let graph = Arc::new(
        Graph::<String>::builder()
            .node("slow", |s: String| async move {
                tokio::time::sleep(Duration::from_secs(3600)).await;
                s
            })
            .entry("slow")
            .build()
            .unwrap(),
    );
    let store = Arc::new(InMemoryCheckpointStore::<String>::new());
    let agora = Agora::builder()
        .runner(move |ctx: JobContext| {
            let (graph, store) = (graph.clone(), store.clone());
            Box::pin(async move {
                let events = graph.stream_job(&ctx.job, "x".into(), &*store, &ctx.agora);
                pump(events, &ctx.sink).await;
            })
        })
        .build();
    let app = agora.router();
    let id = json_of(&app, "POST", "/api/jobs", Some(json!({"request": "wait"}))).await["id"]
        .as_u64()
        .unwrap();
    wait_for_status(&app, id, "running").await;

    json_of(&app, "POST", &format!("/api/jobs/{id}/cancel"), None).await;
    wait_for_status(&app, id, "cancelled").await;
    let (_, sse) = call(&app, "GET", &format!("/api/jobs/{id}/events"), None).await;
    assert!(sse.contains("event: cancelled"));
}

#[tokio::test]
async fn queued_jobs_start_as_running_ones_end_and_can_be_cancelled_first() {
    let agora = Agora::builder()
        .max_running(1)
        .runner(echo_runner())
        .build();
    let app = agora.router();
    let a = json_of(&app, "POST", "/api/jobs", Some(json!({"request": "a"}))).await["id"]
        .as_u64()
        .unwrap();
    let b = json_of(&app, "POST", "/api/jobs", Some(json!({"request": "b"}))).await["id"]
        .as_u64()
        .unwrap();
    let c = json_of(&app, "POST", "/api/jobs", Some(json!({"request": "c"}))).await["id"]
        .as_u64()
        .unwrap();
    json_of(&app, "POST", &format!("/api/jobs/{b}/cancel"), None).await;
    wait_for_status(&app, a, "finished").await;
    wait_for_status(&app, c, "finished").await;
    wait_for_status(&app, b, "cancelled").await;
}

#[tokio::test]
async fn a_step_waiting_for_approval_pauses_the_job_until_a_person_approves() {
    let graph = Arc::new(
        Graph::<String>::builder()
            .node_with_approval(
                "deploy",
                |s: &String| format!("Deploy {s}?"),
                |s: String| async move { format!("deployed {s}") },
            )
            .entry("deploy")
            .build()
            .unwrap(),
    );
    let store = Arc::new(InMemoryCheckpointStore::<String>::new());
    let agora = Agora::builder()
        .runner(move |ctx: JobContext| {
            let (graph, store) = (graph.clone(), store.clone());
            Box::pin(async move {
                let events =
                    graph.stream_job(&ctx.job, ctx.request.request.clone(), &*store, &ctx.agora);
                pump(events, &ctx.sink).await;
            })
        })
        .build();
    let app = agora.router();
    let id = json_of(
        &app,
        "POST",
        "/api/jobs",
        Some(json!({"request": "site", "role": "ops"})),
    )
    .await["id"]
        .as_u64()
        .unwrap();
    wait_for_status(&app, id, "paused").await;

    let groups = json_of(&app, "GET", "/api/approvals", None).await;
    assert_eq!(groups.as_array().unwrap().len(), 1);
    assert_eq!(groups[0]["role"], "ops");
    let item = &groups[0]["items"][0];
    assert_eq!(item["prompt"], "Deploy site?");
    let approval_id = item["id"].as_u64().unwrap();

    json_of(
        &app,
        "POST",
        &format!("/api/approvals/{approval_id}/approve"),
        None,
    )
    .await;
    wait_for_status(&app, id, "finished").await;
    let (_, sse) = call(&app, "GET", &format!("/api/jobs/{id}/events"), None).await;
    assert!(sse.contains("deployed site"));
    assert!(json_of(&app, "GET", "/api/approvals", None)
        .await
        .as_array()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn a_rejected_step_stops_the_job() {
    let graph = Arc::new(
        Graph::<String>::builder()
            .node_with_approval(
                "risky",
                |_: &String| "Run it?".into(),
                |s: String| async move { s },
            )
            .entry("risky")
            .build()
            .unwrap(),
    );
    let store = Arc::new(InMemoryCheckpointStore::<String>::new());
    let agora = Agora::builder()
        .runner(move |ctx: JobContext| {
            let (graph, store) = (graph.clone(), store.clone());
            Box::pin(async move {
                pump(
                    graph.stream_job(&ctx.job, "s".into(), &*store, &ctx.agora),
                    &ctx.sink,
                )
                .await;
            })
        })
        .build();
    let app = agora.router();
    let id = json_of(&app, "POST", "/api/jobs", Some(json!({"request": "r"}))).await["id"]
        .as_u64()
        .unwrap();
    wait_for_status(&app, id, "paused").await;
    let groups = json_of(&app, "GET", "/api/approvals", None).await;
    let aid = groups[0]["items"][0]["id"].as_u64().unwrap();
    json_of(&app, "POST", &format!("/api/approvals/{aid}/reject"), None).await;
    wait_for_status(&app, id, "rejected").await;
}

#[tokio::test]
async fn pending_skills_are_listed_per_role_with_summaries_and_approved_as_a_group() {
    let skills = SkillLibrary::new();
    let skill = |name: &str| {
        Skill::from_markdown(&format!("---\nname: {name}\ndescription: d\n---\nbody")).unwrap()
    };
    skills.propose_inspected(
        "writer",
        skill("tone"),
        Some("Vigil: harmless".into()),
        false,
    );
    skills.propose_inspected(
        "writer",
        skill("style"),
        Some("Vigil: harmless".into()),
        true,
    );
    skills.propose("coder", skill("lint"));
    let mcp = McpLibrary::new();
    mcp.propose("coder", McpServer::http("docs", "https://mcp.example/docs"));

    let agora = Agora::builder()
        .runner(echo_runner())
        .skills(skills.clone())
        .mcp(mcp.clone())
        .build();
    let app = agora.router();

    let groups = json_of(&app, "GET", "/api/approvals", None).await;
    let roles: Vec<&str> = groups
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, ["coder", "writer"]);
    let writer = &groups[1];
    assert_eq!(writer["items"].as_array().unwrap().len(), 2);
    assert_eq!(writer["items"][0]["kind"], "skill");
    assert_eq!(writer["items"][0]["summary"], "Vigil: harmless");
    assert_eq!(writer["items"][1]["unverified"], true);
    assert_eq!(groups[0]["items"][1]["kind"], "mcp_server");

    // Approve one Skill singly.
    let lint = groups[0]["items"][0]["id"].as_u64().unwrap();
    json_of(
        &app,
        "POST",
        &format!("/api/approvals/{lint}/approve"),
        None,
    )
    .await;
    assert_eq!(skills.skills_for_role("coder").len(), 1);
    assert_eq!(skills.pending().len(), 2);

    // Approve the rest of a Role at once.
    json_of(&app, "POST", "/api/roles/writer/approve-all", None).await;
    assert_eq!(skills.skills_for_role("writer").len(), 2);
    assert!(skills.pending().is_empty());
    assert_eq!(mcp.pending().len(), 1, "other groups are left alone");

    json_of(&app, "POST", "/api/roles/coder/approve-all", None).await;
    assert_eq!(mcp.servers_for_role("coder").len(), 1);
    assert!(json_of(&app, "GET", "/api/approvals", None)
        .await
        .as_array()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn budget_overruns_wait_for_approval_and_usage_is_visible() {
    let meter = Arc::new(UsageMeter::new());
    let graph = Arc::new(
        Graph::<String>::builder()
            .metered_node(
                "first",
                "Aria",
                meter.clone(),
                |s: &String| s.clone(),
                |s: String| async move { (s, Usage::new(80, 40, 0.5)) },
            )
            .metered_node(
                "second",
                "Aria",
                meter.clone(),
                |s: &String| s.clone(),
                |s: String| async move { (s, Usage::new(10, 10, 0.1)) },
            )
            .entry("first")
            .edge("first", "second")
            .build()
            .unwrap(),
    );
    let store = Arc::new(InMemoryCheckpointStore::<String>::new());
    let agora = Agora::builder()
        .meter(meter.clone())
        .agents(["Aria"])
        .runner({
            let meter = meter.clone();
            move |ctx: JobContext| {
                let (graph, store, meter) = (graph.clone(), store.clone(), meter.clone());
                Box::pin(async move {
                    // Metered steps ask the Agora only when a Budget is exceeded.
                    let approver = BudgetApprover::new(meter.clone(), ctx.agora.clone());
                    meter.set_job_budget(&ctx.job, Budget::tokens(100));
                    pump(
                        graph.stream_job(&ctx.job, ctx.job.clone(), &*store, &approver),
                        &ctx.sink,
                    )
                    .await;
                })
            }
        })
        .build();
    let app = agora.router();
    let id = json_of(
        &app,
        "POST",
        "/api/jobs",
        Some(json!({"request": "expensive", "role": "analyst"})),
    )
    .await["id"]
        .as_u64()
        .unwrap();
    wait_for_status(&app, id, "paused").await;

    let groups = json_of(&app, "GET", "/api/approvals", None).await;
    assert_eq!(groups[0]["role"], "analyst");
    assert_eq!(groups[0]["items"][0]["kind"], "budget_overrun");

    let usage = json_of(&app, "GET", "/api/usage", None).await;
    assert_eq!(usage["agents"][0]["name"], "Aria");
    assert_eq!(usage["agents"][0]["total_tokens"], 120);
    assert_eq!(usage["jobs"][0]["total_tokens"], 120);
    assert_eq!(usage["jobs"][0]["cost"], 0.5);

    json_of(&app, "POST", "/api/roles/analyst/approve-all", None).await;
    wait_for_status(&app, id, "finished").await;
    let usage = json_of(&app, "GET", "/api/usage", None).await;
    assert_eq!(usage["agents"][0]["total_tokens"], 140);
}

#[tokio::test]
async fn the_index_page_is_served_as_html() {
    let app = Agora::builder().runner(echo_runner()).build().router();
    let res = app
        .clone()
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.headers()["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/html"));
    let html =
        String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(html.contains("Agora"));
}
