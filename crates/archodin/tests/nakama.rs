use archodin::graph::{
    ApprovalDecision, Nakama, Proposal, Provider, Role, Roster, ScriptedApprover, Workload,
};

fn roster() -> Roster {
    let roster = Roster::new();
    roster.add_role(Role::new("Engineer", "Write code"));
    roster.hire("Engineer", "Ada").unwrap();
    roster.hire("Engineer", "Bo").unwrap();
    roster
}

fn nakama() -> Nakama {
    Nakama::new(["Cy", "Di", "Ed"], |work: &str| {
        Role::new("Translator", format!("Handle: {work}"))
            .skill("glossary-lookup")
            .model_chain(vec![Provider::openai_compatible(
                "http://localhost:1",
                "key",
                "any-model",
            )])
    })
    .backlog_threshold(5)
}

#[test]
fn growing_backlog_gets_a_hiring_proposal_naming_agent_and_role() {
    let workload = Workload::new().backlog("Engineer", 8);
    let proposals = nakama().propose(&roster(), &workload);
    assert_eq!(
        proposals,
        vec![Proposal::Hire {
            role: "Engineer".into(),
            agent: "Cy".into()
        }]
    );
    // A small backlog needs no hire.
    let small = Workload::new().backlog("Engineer", 2);
    assert!(nakama().propose(&roster(), &small).is_empty());
}

#[test]
fn idle_agent_gets_a_departure_proposal() {
    let proposals = nakama().propose(&roster(), &Workload::new().idle("Bo"));
    assert_eq!(
        proposals,
        vec![Proposal::Release {
            agent: "Bo".into(),
            role: "Engineer".into()
        }]
    );
    // Unknown agents are ignored.
    assert!(nakama()
        .propose(&roster(), &Workload::new().idle("Ghost"))
        .is_empty());
}

#[test]
fn unmatched_work_gets_a_drafted_role() {
    let proposals = nakama().propose(&roster(), &Workload::new().unmatched("translate contracts"));
    assert_eq!(proposals.len(), 1);
    let Proposal::NewRole { role } = &proposals[0] else {
        panic!("expected a new Role, got {:?}", proposals[0]);
    };
    assert_eq!(role.name(), "Translator");
    assert_eq!(role.instructions(), "Handle: translate contracts");
    assert_eq!(role.skills(), ["glossary-lookup"]);
}

#[tokio::test]
async fn nothing_changes_until_a_person_approves() {
    let roster = roster();
    let workload = Workload::new()
        .backlog("Engineer", 9)
        .idle("Bo")
        .unmatched("translate contracts");
    let nakama = nakama();
    let proposals = nakama.propose(&roster, &workload);
    assert_eq!(proposals.len(), 3);
    // Proposing alone changes nothing.
    assert_eq!(roster.agents().len(), 2);
    assert!(roster.role("Translator").is_none());

    // Rejected and still-pending proposals change nothing.
    let approver = ScriptedApprover::new([
        ApprovalDecision::Rejected,
        ApprovalDecision::Pending,
        ApprovalDecision::Rejected,
    ]);
    let applied = nakama.resolve(&roster, proposals.clone(), &approver).await;
    assert!(applied.is_empty());
    assert_eq!(approver.requests().len(), 3);
    assert_eq!(roster.agents().len(), 2);
    assert!(roster.role("Translator").is_none());

    // Approved ones take effect.
    let approver = ScriptedApprover::new([ApprovalDecision::Approved; 3]);
    let applied = nakama.resolve(&roster, proposals, &approver).await;
    assert_eq!(applied.len(), 3);
    let names: Vec<_> = roster
        .agents()
        .iter()
        .map(|a| a.name().to_string())
        .collect();
    assert_eq!(names, ["Ada", "Cy"]);
    assert!(roster.role("Translator").is_some());
    assert!(approver.requests()[0].prompt.contains("Cy"));
}

#[test]
fn a_name_already_proposed_is_not_proposed_again() {
    let workload = Workload::new().backlog("Engineer", 9);
    let n = nakama();
    let roster = roster();
    let first = n.propose(&roster, &workload);
    let second = n.propose(&roster, &workload);
    assert_ne!(first, second);
}
