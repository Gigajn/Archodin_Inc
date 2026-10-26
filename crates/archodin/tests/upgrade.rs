use archodin::graph::{
    ApprovalDecision, Budget, Explorador, Nakama, RepoInfo, RepoRef, Role, Roster,
    ScriptedApprover, ScriptedGitHub, Upgrade, UpgradeError, Usage, UsageMeter,
    VersionedDepartment,
};

fn fake() -> ScriptedGitHub {
    ScriptedGitHub::new("kudo").repository(RepoInfo {
        repo: RepoRef::new("acme", "widgets"),
        description: None,
        default_branch: "main".into(),
    })
}

fn roster() -> Roster {
    let roster = Roster::new();
    roster.add_role(Role::new("Contributor", "Fix issues"));
    roster.add_role(Role::new("Reviewer", "Review changes"));
    roster
}

fn nakama() -> Nakama {
    Nakama::new(["Ada", "Bo", "Cy", "Di"], |_: &str| Role::new("X", "x"))
}

fn explorador() -> Explorador {
    Explorador::new(["open-source"])
}

async fn running(roster: &Roster, meter: &UsageMeter) -> VersionedDepartment {
    let charter = explorador()
        .charter_from_link(&fake(), "acme/widgets")
        .await
        .unwrap()
        .with_budget(Budget::tokens(1000));
    let approver = ScriptedApprover::new([ApprovalDecision::Approved]);
    let department = explorador()
        .approve(charter, &approver, roster, &nakama(), meter)
        .await
        .unwrap()
        .unwrap();
    VersionedDepartment::new(department)
}

fn upgrade() -> Upgrade {
    Upgrade::new()
        .add_agents("Contributor", 1)
        .add_agents("Reviewer", 1)
        .skill("triage-issues")
        .graph("review-then-merge")
        .budget(Budget::tokens(5000))
}

#[tokio::test]
async fn an_upgrade_charter_shows_what_changes_and_changes_nothing() {
    let (roster, meter) = (roster(), UsageMeter::new());
    let dept = running(&roster, &meter).await;
    assert_eq!(dept.version(), 1);

    let charter = explorador().propose_upgrade(&dept, upgrade());
    let text = charter.describe();
    for needle in [
        "widgets",
        "version 1",
        "Reviewer",
        "triage-issues",
        "review-then-merge",
        "5000",
    ] {
        assert!(text.contains(needle), "missing {needle} in {text}");
    }
    assert!(charter.changes().len() >= 4);
    assert_eq!(charter.department(), "widgets");

    assert_eq!(dept.version(), 1);
    assert_eq!(roster.agents().len(), 1);
    assert_eq!(dept.setup().budget(), Budget::tokens(1000));
    assert!(dept.setup().skills().is_empty());
}

#[tokio::test]
async fn rejected_or_pending_upgrades_change_nothing() {
    let (roster, meter) = (roster(), UsageMeter::new());
    let dept = running(&roster, &meter).await;
    for decision in [ApprovalDecision::Rejected, ApprovalDecision::Pending] {
        let approver = ScriptedApprover::new([decision]);
        let charter = explorador().propose_upgrade(&dept, upgrade());
        let done = explorador()
            .approve_upgrade(charter, &approver, &dept, &roster, &nakama(), &meter)
            .await
            .unwrap();
        assert_eq!(done, None);
        assert_eq!(approver.requests()[0].node, "upgrade");
        assert_eq!(dept.version(), 1);
        assert_eq!(roster.agents().len(), 1);
    }
}

#[tokio::test]
async fn approval_makes_the_next_version_for_new_jobs_while_running_jobs_finish_on_the_old_one() {
    let (roster, meter) = (roster(), UsageMeter::new());
    let dept = running(&roster, &meter).await;
    let before = dept.start_job();
    assert_eq!(before.version(), 1);

    let approver = ScriptedApprover::new([ApprovalDecision::Approved]);
    let charter = explorador().propose_upgrade(&dept, upgrade());
    let done = explorador()
        .approve_upgrade(charter, &approver, &dept, &roster, &nakama(), &meter)
        .await
        .unwrap();
    assert_eq!(done, Some(2));

    // The Job that was already running keeps the old setup.
    assert_eq!(before.version(), 1);
    assert_eq!(before.setup().budget(), Budget::tokens(1000));
    assert_eq!(before.setup().graph(), None);
    // New Jobs get the new one.
    let after = dept.start_job();
    assert_eq!(after.version(), 2);
    assert_eq!(after.setup().budget(), Budget::tokens(5000));
    assert_eq!(after.setup().graph(), Some("review-then-merge"));
    assert_eq!(after.setup().skills(), ["triage-issues"]);
    assert_eq!(
        after.setup().roles(),
        [("Contributor".to_string(), 2), ("Reviewer".to_string(), 1)]
    );
    // Nakama hired the extra Agents.
    assert_eq!(dept.department().agents().len(), 3);
    assert_eq!(roster.agents().len(), 3);
    assert_eq!(dept.history().len(), 2);
}

#[tokio::test]
async fn rolling_back_restores_the_previous_version_for_new_jobs() {
    let (roster, meter) = (roster(), UsageMeter::new());
    let dept = running(&roster, &meter).await;
    assert_eq!(dept.rollback(&meter), Err(UpgradeError::NothingToRollBack));

    let approver = ScriptedApprover::new([ApprovalDecision::Approved, ApprovalDecision::Approved]);
    for _ in 0..2 {
        let charter = explorador().propose_upgrade(&dept, Upgrade::new().skill("s"));
        explorador()
            .approve_upgrade(charter, &approver, &dept, &roster, &nakama(), &meter)
            .await
            .unwrap();
    }
    let charter = explorador().propose_upgrade(&dept, upgrade());
    let stale = charter.clone();
    let approver = ScriptedApprover::new([ApprovalDecision::Approved]);
    explorador()
        .approve_upgrade(charter, &approver, &dept, &roster, &nakama(), &meter)
        .await
        .unwrap();
    assert_eq!(dept.version(), 4);
    let running_job = dept.start_job();

    assert_eq!(dept.rollback(&meter), Ok(3));
    assert_eq!(dept.version(), 3);
    assert_eq!(dept.start_job().setup().budget(), Budget::tokens(1000));
    assert_eq!(dept.start_job().setup().graph(), None);
    // A Job started before the rollback finishes on the version it began with.
    assert_eq!(running_job.version(), 4);
    // History is kept, and the next upgrade gets a fresh version number.
    assert_eq!(dept.history().len(), 4);
    let charter = explorador().propose_upgrade(&dept, Upgrade::new().skill("again"));
    let approver = ScriptedApprover::new([ApprovalDecision::Approved]);
    let done = explorador()
        .approve_upgrade(charter, &approver, &dept, &roster, &nakama(), &meter)
        .await
        .unwrap();
    assert_eq!(done, Some(5));

    // A Charter drafted against an older version is refused.
    let approver = ScriptedApprover::new([ApprovalDecision::Approved]);
    let err = explorador()
        .approve_upgrade(stale, &approver, &dept, &roster, &nakama(), &meter)
        .await;
    assert!(matches!(err, Err(UpgradeError::Stale { .. })));
    assert!(approver.requests().is_empty());
}

#[tokio::test]
async fn the_department_budget_on_the_meter_follows_the_current_version() {
    let (roster, meter) = (roster(), UsageMeter::new());
    let dept = running(&roster, &meter).await;
    let key = dept.department().budget_key();
    meter.record("j", &key, Usage::new(1500, 0, 0.0));
    assert!(meter.exceeded("", &key).is_some());

    let approver = ScriptedApprover::new([ApprovalDecision::Approved]);
    let charter = explorador().propose_upgrade(&dept, Upgrade::new().budget(Budget::tokens(5000)));
    explorador()
        .approve_upgrade(charter, &approver, &dept, &roster, &nakama(), &meter)
        .await
        .unwrap();
    assert!(meter.exceeded("", &key).is_none());

    dept.rollback(&meter).unwrap();
    assert!(meter.exceeded("", &key).is_some());
}
