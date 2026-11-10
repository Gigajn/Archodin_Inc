//! Role-owned Skills held by Codex until a person approves them (ADR 0004).

use archodin::graph::{
    ApprovalDecision, Role, Roster, ScriptedApprover, Skill, SkillError, SkillKind, SkillLibrary,
};

const FLOW_MD: &str = "---\nname: release\ndescription: Ship a release\nkind: flow\nuses: tidy\n---\nRun the checks, then tag.\n";
const TIDY_MD: &str =
    "---\nname: tidy\ndescription: Keep diffs small\nkind: discipline\n---\nKeep diffs small.\n";

fn setup() -> (Roster, SkillLibrary) {
    let roster = Roster::new();
    roster.add_role(Role::new("Engineer", "Write code"));
    roster.add_role(Role::new("Designer", "Draw"));
    (roster, SkillLibrary::new())
}

async fn approve(lib: &SkillLibrary, n: usize) {
    let a = ScriptedApprover::new(vec![ApprovalDecision::Approved; n]);
    lib.review_pending(&a).await;
}

#[test]
fn parses_skill_md() {
    let s = Skill::from_markdown(FLOW_MD).unwrap();
    assert_eq!(s.name(), "release");
    assert_eq!(s.kind(), SkillKind::Flow);
    assert_eq!(s.uses(), ["tidy"]);
    assert!(s.body().contains("tag"));
    assert!(matches!(
        Skill::from_markdown("no frontmatter"),
        Err(SkillError::InvalidSkill(_))
    ));
}

#[tokio::test]
async fn approved_skill_reaches_every_agent_of_the_role() {
    let (roster, lib) = setup();
    let ada = roster.hire("Engineer", "Ada").unwrap();
    let bo = roster.hire("Engineer", "Bo").unwrap();
    let cy = roster.hire("Designer", "Cy").unwrap();
    lib.propose("Engineer", Skill::from_markdown(TIDY_MD).unwrap());
    assert!(lib.skills_for_agent(&ada).is_empty());
    approve(&lib, 1).await;
    assert_eq!(lib.skills_for_agent(&ada)[0].name(), "tidy");
    assert_eq!(lib.skills_for_agent(&bo)[0].name(), "tidy");
    assert!(lib.skills_for_agent(&cy).is_empty());
}

#[tokio::test]
async fn shared_skill_reaches_other_roles_unshared_does_not() {
    let (_, lib) = setup();
    lib.propose("Engineer", Skill::from_markdown(TIDY_MD).unwrap());
    approve(&lib, 1).await;
    assert!(lib.skills_for_role("Designer").is_empty());
    lib.share("Engineer", "tidy").unwrap();
    assert_eq!(lib.skills_for_role("Designer").len(), 1);
    lib.unshare("Engineer", "tidy").unwrap();
    assert!(lib.skills_for_role("Designer").is_empty());
    assert_eq!(
        lib.share("Engineer", "nope"),
        Err(SkillError::UnknownSkill("nope".into()))
    );
}

#[tokio::test]
async fn flow_may_use_discipline_but_not_another_flow() {
    let (_, lib) = setup();
    let other = FLOW_MD
        .replace("name: release", "name: deploy")
        .replace("uses: tidy", "uses:");
    for md in [FLOW_MD, TIDY_MD, other.as_str()] {
        lib.propose("Engineer", Skill::from_markdown(md).unwrap());
    }
    approve(&lib, 3).await;
    let mut run = lib.start_flow("Engineer", "release").unwrap();
    assert_eq!(run.use_skill("tidy").unwrap().name(), "tidy");
    assert_eq!(
        run.use_skill("deploy").err(),
        Some(SkillError::FlowCannotStartFlow("deploy".into()))
    );
    assert_eq!(
        lib.start_flow("Engineer", "tidy").err(),
        Some(SkillError::NotAFlow("tidy".into()))
    );
}

#[tokio::test]
async fn pending_is_unused_and_rejection_leaves_no_trace() {
    let (_, lib) = setup();
    lib.propose("Engineer", Skill::from_markdown(TIDY_MD).unwrap());
    assert!(lib.skills_for_role("Engineer").is_empty());
    assert_eq!(lib.pending().len(), 1);
    // No answer yet: stays pending.
    lib.review_pending(&ScriptedApprover::new([])).await;
    assert_eq!(lib.pending().len(), 1);
    assert!(lib.skills_for_role("Engineer").is_empty());
    let rejector = ScriptedApprover::new([ApprovalDecision::Rejected]);
    lib.review_pending(&rejector).await;
    assert!(lib.pending().is_empty());
    assert!(lib.skills_for_role("Engineer").is_empty());
    assert!(lib.versions("Engineer", "tidy").is_empty());
    assert_eq!(rejector.requests().len(), 1);
}

#[tokio::test]
async fn changed_skill_waits_while_old_version_stays_and_rollback_restores_it() {
    let (_, lib) = setup();
    lib.propose("Engineer", Skill::from_markdown(TIDY_MD).unwrap());
    approve(&lib, 1).await;
    let v2 = TIDY_MD.replace("Keep diffs small.\n", "Keep diffs tiny.\n");
    lib.propose("Engineer", Skill::from_markdown(&v2).unwrap());
    assert!(lib.skills_for_role("Engineer")[0].body().contains("small"));
    approve(&lib, 1).await;
    assert!(lib.skills_for_role("Engineer")[0].body().contains("tiny"));
    assert_eq!(lib.versions("Engineer", "tidy"), [1, 2]);
    lib.rollback("Engineer", "tidy").unwrap();
    assert!(lib.skills_for_role("Engineer")[0].body().contains("small"));
    assert_eq!(
        lib.rollback("Engineer", "tidy"),
        Err(SkillError::NothingToRollBack("tidy".into()))
    );
}

#[tokio::test]
async fn scripts_stay_disabled_until_approved_separately() {
    let (_, lib) = setup();
    let skill = Skill::from_markdown(TIDY_MD)
        .unwrap()
        .script("fmt.sh", "cargo fmt");
    lib.propose("Engineer", skill);
    approve(&lib, 1).await;
    assert_eq!(
        lib.authorized_script("Engineer", "tidy", "fmt.sh"),
        Err(SkillError::ScriptsNotApproved("tidy".into()))
    );
    let a = ScriptedApprover::new([ApprovalDecision::Approved]);
    lib.review_scripts("Engineer", "tidy", &a).await.unwrap();
    assert!(a.requests()[0].prompt.contains("scripts"));
    assert_eq!(
        lib.authorized_script("Engineer", "tidy", "fmt.sh").unwrap(),
        "cargo fmt"
    );
    // A changed version needs its scripts approved again.
    let v2 = Skill::from_markdown(TIDY_MD)
        .unwrap()
        .script("fmt.sh", "rm -rf /");
    lib.propose("Engineer", v2);
    approve(&lib, 1).await;
    assert!(lib.authorized_script("Engineer", "tidy", "fmt.sh").is_err());
}

#[test]
fn skill_names_must_be_short_lowercase_and_hyphenated() {
    let md = |name: &str| format!("---\nname: {name}\ndescription: d\n---\nbody\n");
    for bad in [
        "Bad_Name!",
        "has space",
        "UPPER",
        "9lives",
        "-leading",
        "trailing-",
    ] {
        assert!(
            matches!(
                Skill::from_markdown(&md(bad)),
                Err(SkillError::InvalidSkill(_))
            ),
            "{bad} must be rejected"
        );
    }
    assert!(matches!(
        Skill::from_markdown(&md(&"a".repeat(65))),
        Err(SkillError::InvalidSkill(_))
    ));
    assert!(Skill::from_markdown(&md("ok-name-1")).is_ok());
    assert!(Skill::from_markdown(&md(&"a".repeat(64))).is_ok());
    // A `uses` entry must be a valid skill name too.
    assert!(matches!(
        Skill::from_markdown("---\nname: tidy\ndescription: d\nuses: Bad Name\n---\nbody\n"),
        Err(SkillError::InvalidSkill(_))
    ));
}

#[test]
fn skill_text_has_length_limits() {
    let md = format!(
        "---\nname: tidy\ndescription: {}\n---\nbody\n",
        "d".repeat(513)
    );
    assert!(matches!(
        Skill::from_markdown(&md),
        Err(SkillError::InvalidSkill(_))
    ));
    // The parsed body keeps one trailing newline, so 65_535 `b`s are exactly at
    // the 65_536 limit and 70_000 are over it.
    let md = format!(
        "---\nname: tidy\ndescription: d\n---\n{}\n",
        "b".repeat(70_000)
    );
    assert!(matches!(
        Skill::from_markdown(&md),
        Err(SkillError::InvalidSkill(_))
    ));
    let md = format!(
        "---\nname: tidy\ndescription: {}\n---\n{}\n",
        "d".repeat(512),
        "b".repeat(65_535)
    );
    assert!(Skill::from_markdown(&md).is_ok());
}

#[test]
fn an_uninspected_proposal_is_recorded_as_unverified() {
    // A Skill queued without Vigil's inspection must not be recorded as if it
    // had been checked: no summary, and flagged unverified.
    let (_, lib) = setup();
    lib.propose("Engineer", Skill::from_markdown(TIDY_MD).unwrap());
    let pending = lib.pending();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].unverified);
    assert_eq!(pending[0].summary, None);
}
