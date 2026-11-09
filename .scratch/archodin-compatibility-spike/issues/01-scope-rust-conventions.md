# 01: Scope Rust conventions without losing legacy guidance

**What to build:** Generic Archodin Rust guidance that cannot accidentally import Basileia's employee model or legacy Python routing error contract, while preserving those rules for their actual legacy scope.

**Blocked by:** None (can start immediately; owner-approved spike).

**Status:** in-progress

- [x] Preserve the existing Skill name and snapshot its baseline before editing. Separate generic Rust rules from application/legacy rules without merging unrelated Skills.
- [x] Clarify that crate examples are not authorization to add dependencies or select the spike's integration library.
- [x] Use skill-creator to compare baseline and updated guidance with the approved Provider, typed Tool/async, and legacy router evaluation prompts. Preserve offline tests, typed errors, Tokio deadlines/cancellation, and legacy parity where applicable.
- [x] Record available timing/usage and observed scope correctness; unavailable metrics remain unknown, and static checks are not represented as behavioral evaluations.
- [ ] Generate a static evaluation viewer for owner review before accepting the update. Keep evaluation outputs and credentials out of the Skill instructions.
- [x] Run the repository gate and record its full output in Comments before done.

## Comments

Owner approved this ticket and the spike sequence. This is scoped Skill maintenance, not permission to implement the Provider, alter legacy error behavior, or publish repositories.


### Candidate preparation and evaluation (pending owner review)

The active Skill is unchanged and byte-identical to the pre-edit snapshot. A candidate with the same Skill name separates generic Rust rules from conditional legacy/application guidance. It preserves typed errors, offline seams, Tokio deadlines/cancellation and cleanup, while marking crate examples as conditional rather than dependency selections. TDD, clean-code and token-efficiency remain separate Skills.

A static scope-routing check failed on the baseline before candidate preparation and passed afterward. This checks document structure only; it is not runtime TDD or behavioral compatibility proof. Six isolated model runs then answered the three approved scenarios, once per configuration; an independent grader evaluated substantive design assertions with quoted evidence.

Results: Provider 4/4 vs 4/4; Tool/async 5/5 vs 4/5; legacy router 4/4 vs 4/4 (candidate vs baseline). The baseline Tool plan did not require validation before effects. Most assertions do not discriminate these samples; the single run per scenario does not establish repeatable improvement. No proposed contract was compiled or executed. Legacy results assess preservation/characterization planning, not verified Python parity.

Zed returned no token or timing metrics; values remain null/unknown, not zero. Executor model identity is also unknown because the configured default was used. The installed skill-creator viewer/scripts directories were empty, so the official upstream generator/template and aggregator were downloaded into the evaluation workspace, inspected and source-hashed. Only static generation was executed. Manual schema-compatible aggregation avoids the upstream aggregator's zero/output-character substitutions for unavailable metrics. Generated HTML has external font/CDN resources removed for offline review. No server or project upload was involved.

Evidence workspace: `.agents/skills/rust-conventions-workspace/`. Because that directory is Git-ignored, a preservation snapshot (baseline, candidate, prompts, all six answers/transcripts/gradings, benchmark, tooling provenance and static viewer) is archived at `.scratch/archodin-compatibility-spike/rust-conventions-review/`. This is review evidence, not a second installed Skill. Start with its `README.md`, `iteration-1/benchmark.md` and `iteration-1/review.html`.

**Owner review:** pending. Viewer generated; acceptance is intentionally unchecked until actual review. Do not install the candidate, mark this ticket done or start its dependent Provider ticket yet. No owner consent is manufactured by this record.

### Repository gate

`./scripts/check.sh` passed. Full output is saved in the feature's `skill-maintenance-gate-output.txt` and pasted below. Passing the existing runtime suite does not prove the proposed Provider/Tool contracts exist.

```text
==> fmt
==> frontier
test_closed_without_completion_does_not_satisfy_a_prerequisite (test_frontier.FrontierCliTests.test_closed_without_completion_does_not_satisfy_a_prerequisite) ... ok
test_comment_examples_cannot_supply_missing_metadata (test_frontier.FrontierCliTests.test_comment_examples_cannot_supply_missing_metadata) ... ok
test_dependency_cycles_are_reported_as_blocked (test_frontier.FrontierCliTests.test_dependency_cycles_are_reported_as_blocked) ... ok
test_done_marker_cannot_hide_invalid_or_cyclic_metadata (test_frontier.FrontierCliTests.test_done_marker_cannot_hide_invalid_or_cyclic_metadata) ... ok
test_duplicate_metadata_fields_fail_closed (test_frontier.FrontierCliTests.test_duplicate_metadata_fields_fail_closed) ... ok
test_duplicate_prerequisite_identity_is_not_treated_as_done (test_frontier.FrontierCliTests.test_duplicate_prerequisite_identity_is_not_treated_as_done) ... ok
test_empty_or_conflicting_legacy_review_is_not_approval (test_frontier.FrontierCliTests.test_empty_or_conflicting_legacy_review_is_not_approval) ... ok
test_empty_tracker_has_consistent_json_and_human_output (test_frontier.FrontierCliTests.test_empty_tracker_has_consistent_json_and_human_output) ... ok
test_human_and_json_reports_explain_the_same_eligibility (test_frontier.FrontierCliTests.test_human_and_json_reports_explain_the_same_eligibility) ... ok
test_legacy_work_requires_explicit_owner_revalidation (test_frontier.FrontierCliTests.test_legacy_work_requires_explicit_owner_revalidation) ... ok
test_only_ready_for_agent_tickets_are_offered (test_frontier.FrontierCliTests.test_only_ready_for_agent_tickets_are_offered) ... ok
test_parenthesized_notes_do_not_create_dependencies (test_frontier.FrontierCliTests.test_parenthesized_notes_do_not_create_dependencies) ... ok
test_qualified_blocker_uses_the_named_feature (test_frontier.FrontierCliTests.test_qualified_blocker_uses_the_named_feature) ... ok
test_sentence_terminated_dependency_is_parsed (test_frontier.FrontierCliTests.test_sentence_terminated_dependency_is_parsed) ... ok
test_unsupported_dependency_input_fails_closed (test_frontier.FrontierCliTests.test_unsupported_dependency_input_fails_closed) ... ok

----------------------------------------------------------------------
Ran 15 tests in 2.466s

OK
==> clippy
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.19s
==> test
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.17s
     Running unittests src/lib.rs (target/debug/deps/aahc_core-3656c1a115689065)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/main.rs (target/debug/deps/aahc_core-7e75ddca3e06342d)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/config.rs (target/debug/deps/config-b63ec7559c776cff)

running 5 tests
test bind_address_is_configurable ... ok
test bind_address_defaults_to_localhost ... ok
test database_path_is_read_from_the_environment ... ok
test invalid_bind_address_is_a_typed_error_naming_the_variable ... ok
test missing_database_path_is_a_typed_error_naming_the_variable ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/db_schema.rs (target/debug/deps/db_schema-30b5a109867f54d7)

running 6 tests
test init_creates_every_table_the_spec_requires ... ok
test employee_table_carries_the_spec_columns_and_check_constraints ... ok
test router_pool_is_seeded_with_the_eleven_free_lanes ... ok
test unique_lane_index_prevents_duplicate_lanes ... ok
test seeding_is_idempotent_across_runs ... ok
test schema_matches_the_frozen_python_oracle ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s

     Running tests/health.rs (target/debug/deps/health-d2000292d7eea39b)

running 3 tests
test unknown_route_is_not_found ... ok
test health_reports_the_service_is_ok ... ok
test health_reports_the_service_version ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests aahc_core

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

==> all gates passed
```
