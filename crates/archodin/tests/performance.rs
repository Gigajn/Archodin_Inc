use archodin::graph::{
    ApprovalDecision, Nakama, PerformanceLedger, Role, Roster, ScriptedApprover, Usage, Workload,
};

fn roster() -> Roster {
    let roster = Roster::new();
    roster.add_role(Role::new("Engineer", "Write code"));
    roster.hire("Engineer", "Ada").unwrap();
    roster.hire("Engineer", "Bo").unwrap();
    roster
}

fn nakama() -> Nakama {
    Nakama::new(["Cy", "Di"], |w: &str| Role::new("Translator", w)).backlog_threshold(5)
}

#[test]
fn every_job_updates_the_agents_performance_record() {
    let ledger = PerformanceLedger::new();
    ledger.job_finished("job-1", "Ada", Usage::new(100, 50, 0.30));
    ledger.job_finished("job-2", "Ada", Usage::new(10, 10, 0.10));
    ledger.correction("Ada");
    ledger.budget_overrun("Ada");
    ledger.qadi_decision("Ada", ["Bo"]);
    ledger.qadi_decision("Bo", ["Ada"]);
    ledger.qadi_decision("Ada", ["Bo"]);

    let report = ledger.report();
    let ada = report.agent("Ada").unwrap();
    assert_eq!(ada.jobs_finished, 2);
    assert_eq!(ada.corrections, 1);
    assert_eq!(ada.budget_overruns, 1);
    assert_eq!((ada.decisions_won, ada.decisions_lost), (2, 1));
    assert!((ada.cost_per_job().unwrap() - 0.20).abs() < 1e-9);

    let bo = report.agent("Bo").unwrap();
    assert_eq!((bo.decisions_won, bo.decisions_lost), (1, 2));
    assert_eq!(bo.cost_per_job(), None);
    assert!(report.agent("Ghost").is_none());

    // Sorted by name, ready for the Agora to show next to name and Role.
    let names: Vec<_> = report.agents().iter().map(|a| a.agent.as_str()).collect();
    assert_eq!(names, ["Ada", "Bo"]);
    assert!(ada.summary().contains("2 Jobs"));
}

#[test]
fn nakama_cites_the_figures_behind_each_proposal() {
    let ledger = PerformanceLedger::new();
    ledger.job_finished("j1", "Bo", Usage::new(1, 1, 1.0));
    ledger.correction("Bo");
    ledger.correction("Bo");
    ledger.job_finished("j2", "Ada", Usage::new(1, 1, 0.1));

    let workload = Workload::new().backlog("Engineer", 9).idle("Bo");
    let cited = nakama().propose_cited(&roster(), &workload, &ledger.report());
    assert_eq!(cited.len(), 2);

    // The hire cites the Role's current Agents; the departure cites that Agent.
    let hire = &cited[0];
    assert_eq!(hire.figures.len(), 2);
    assert!(hire.describe().contains("Hire Cy as Engineer"));
    assert!(hire.describe().contains("Ada: 1 Jobs"));
    let release = &cited[1];
    assert_eq!(release.figures.len(), 1);
    assert!(release.describe().contains("2 corrections"));
}

#[tokio::test]
async fn the_approval_prompt_carries_the_figures() {
    let ledger = PerformanceLedger::new();
    ledger.job_finished("j1", "Bo", Usage::new(1, 1, 1.0));
    let roster = roster();
    let n = nakama();
    let cited = n.propose_cited(&roster, &Workload::new().idle("Bo"), &ledger.report());
    let approver = ScriptedApprover::new([ApprovalDecision::Approved]);
    let applied = n.resolve_cited(&roster, cited, &approver).await;
    assert_eq!(applied.len(), 1);
    assert!(approver.requests()[0].prompt.contains("Bo: 1 Jobs"));
    assert!(roster.agent("Bo").is_none());
}
