//! The Sandbox: where Agents run code and approved Skill scripts (issue #26).
//!
//! Real-container tests are `#[ignore]`d; run them on a machine with docker or
//! podman: `ARCHODIN_CONTAINER_RUNTIME=docker cargo test -p archodin --test sandbox -- --ignored`
//! (needs the `alpine:3` image, pulled beforehand because the Sandbox has no network).

use std::sync::Arc;
use std::time::Duration;

use archodin::graph::{
    run_skill_script, ApprovalDecision, ContainerSandbox, Limit, Sandbox, SandboxError,
    SandboxInput, SandboxOutput, SandboxPolicy, SandboxTool, ScriptedApprover, ScriptedSandbox,
    Skill, SkillLibrary, SkillScriptError, TypedTool,
};

fn ok(stdout: &str) -> Result<SandboxOutput, SandboxError> {
    Ok(SandboxOutput {
        stdout: stdout.into(),
        stderr: String::new(),
        exit_code: 0,
    })
}

#[tokio::test]
async fn a_tool_runs_a_command_in_the_sandbox_and_returns_its_output() {
    let sandbox = Arc::new(ScriptedSandbox::new([ok("hello\n")]));
    let tool = SandboxTool::new(sandbox.clone());
    assert_eq!(tool.name(), "run_command");
    let out = tool
        .call(SandboxInput {
            command: "echo hello".into(),
        })
        .await
        .unwrap();
    assert!(out.contains("hello"));
    assert!(out.contains("exit code: 0"));
    assert_eq!(sandbox.commands(), ["echo hello"]);
}

#[tokio::test]
async fn a_failing_command_is_shown_to_the_agent_not_an_error() {
    let sandbox = Arc::new(ScriptedSandbox::new([Ok(SandboxOutput {
        stdout: String::new(),
        stderr: "boom".into(),
        exit_code: 2,
    })]));
    let out = SandboxTool::new(sandbox)
        .call(SandboxInput {
            command: "x".into(),
        })
        .await
        .unwrap();
    assert!(out.contains("exit code: 2") && out.contains("boom"));
}

#[tokio::test]
async fn a_limit_that_is_hit_is_reported() {
    for limit in [Limit::Time, Limit::Memory, Limit::Network] {
        let sandbox = Arc::new(ScriptedSandbox::new([Err(SandboxError::LimitHit(limit))]));
        let err = SandboxTool::new(sandbox)
            .call(SandboxInput {
                command: "x".into(),
            })
            .await
            .unwrap_err();
        assert_eq!(err, SandboxError::LimitHit(limit));
        assert!(err.to_string().contains(&limit.to_string()));
    }
}

#[test]
fn default_policy_has_no_network_and_finite_limits() {
    let p = SandboxPolicy::default();
    assert!(!p.network());
    assert!(p.cpus() > 0.0 && p.memory_mb() > 0 && p.timeout() > Duration::ZERO);
}

#[test]
fn container_command_isolates_the_host_and_applies_limits() {
    let policy = SandboxPolicy::default()
        .with_cpus(0.5)
        .with_memory_mb(128)
        .with_timeout(Duration::from_secs(7));
    let sb = ContainerSandbox::docker(policy);
    let args = sb.run_args("job-1", "echo hi");
    let joined = args.join(" ");
    assert_eq!(args[0], "run");
    assert!(joined.contains("--network none"));
    assert!(joined.contains("--cpus 0.5"));
    assert!(joined.contains("--memory 128m"));
    assert!(joined.contains("--memory-swap 128m"));
    assert!(joined.contains("--read-only"));
    assert!(joined.contains("--cap-drop ALL"));
    assert!(joined.contains("no-new-privileges"));
    assert!(joined.contains("--pids-limit"));
    assert!(joined.ends_with("sh -c echo hi"));
    // No host files or secrets: no mounts, no inherited environment.
    for forbidden in ["-v", "--volume", "--mount", "-e", "--env", "--env-file"] {
        assert!(!args.iter().any(|a| a == forbidden), "{forbidden}");
    }
}

#[test]
fn network_is_only_on_when_the_policy_says_so() {
    let sb = ContainerSandbox::podman(SandboxPolicy::default().with_network(true));
    assert!(!sb
        .run_args("j", "true")
        .join(" ")
        .contains("--network none"));
    assert_eq!(sb.runtime(), "podman");
}

#[tokio::test]
async fn a_missing_runtime_is_reported_not_run_on_the_host() {
    let sb = ContainerSandbox::with_runtime("archodin-no-such-runtime", SandboxPolicy::default());
    let err = sb.run("echo hi").await.unwrap_err();
    assert!(matches!(err, SandboxError::RuntimeUnavailable(_)));
}

#[tokio::test]
async fn approved_skill_scripts_run_only_inside_the_sandbox() {
    let lib = SkillLibrary::new();
    let skill = Skill::from_markdown(
        "---\nname: tidy\ndescription: Keep diffs small\nkind: discipline\n---\nKeep.\n",
    )
    .unwrap()
    .script("fmt.sh", "cargo fmt");
    lib.propose("Engineer", skill);
    let a = ScriptedApprover::new(vec![ApprovalDecision::Approved; 1]);
    lib.review_pending(&a).await;
    let sandbox = ScriptedSandbox::new([ok("done")]);

    // Not yet approved: nothing reaches the sandbox.
    let err = run_skill_script(&lib, &sandbox, "Engineer", "tidy", "fmt.sh")
        .await
        .unwrap_err();
    assert!(matches!(err, SkillScriptError::Skill(_)));
    assert!(sandbox.commands().is_empty());

    let a = ScriptedApprover::new([ApprovalDecision::Approved]);
    lib.review_scripts("Engineer", "tidy", &a).await.unwrap();
    let out = run_skill_script(&lib, &sandbox, "Engineer", "tidy", "fmt.sh")
        .await
        .unwrap();
    assert_eq!(out.stdout, "done");
    assert_eq!(sandbox.commands(), ["cargo fmt"]);
}

fn real() -> ContainerSandbox {
    let rt = std::env::var("ARCHODIN_CONTAINER_RUNTIME").unwrap_or_else(|_| "docker".into());
    ContainerSandbox::with_runtime(
        rt,
        SandboxPolicy::default().with_timeout(Duration::from_secs(5)),
    )
}

#[tokio::test]
#[ignore = "needs docker or podman and the alpine:3 image"]
async fn real_container_runs_without_network_host_files_or_secrets() {
    let sb = real();
    let out = sb.run("echo hi").await.unwrap();
    assert_eq!(out.stdout.trim(), "hi");
    let out = sb.run("ls /home; echo $HOME_SECRET").await.unwrap();
    assert!(out.stdout.trim().is_empty());
    let out = sb.run("wget -T 2 -q -O- http://1.1.1.1").await.unwrap();
    assert_ne!(out.exit_code, 0);
    let out = sb.run("touch /x").await.unwrap();
    assert_ne!(out.exit_code, 0);
}

#[tokio::test]
#[ignore = "needs docker or podman and the alpine:3 image"]
async fn real_container_reports_time_limit() {
    let sb = real();
    let err = sb.run("sleep 30").await.unwrap_err();
    assert_eq!(err, SandboxError::LimitHit(Limit::Time));
}
