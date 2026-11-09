# 01: Enforce eligible frontier selection and qualified dependencies

**What to build:** The owner and agent can identify genuinely ready implementation work through the frontier CLI, including qualified cross-feature blockers, without admitting unvalidated legacy or malformed dependency input.

**Blocked by:** None (owner approved the roadmap and supporting slices).

**Status:** done

- [x] Only ready-for-agent, unblocked tickets are offered as new implementation work; triage, needs-info, human-only and in-progress work are excluded.
- [x] Legacy AAHC work requires an explicit owner-approved revalidation record rather than an old ready label.
- [x] Same-feature numbers and qualified feature#number dependencies are supported without confusing identical numbers in different features.
- [x] Missing, malformed, ambiguous and cyclic prerequisites fail closed with visible reasons instead of being silently ignored.
- [x] Human-readable and JSON results agree, including an empty tracker; histories and existing ticket content remain untouched.
- [x] Public CLI fixture tests demonstrate incremental red→green behavior; record targeted tests and full repository gate output before done.

## Comments

### Approved seam and scope

The owner approved the roadmap/supporting slices and requested TDD. The seam is the public frontier CLI invoked with a fixture tracker root; observations are exit status, JSON and human-readable eligibility/diagnostics. It catches work-selection behavior, not runtime/Provider correctness. This ticket does not authorize executing AAHC tickets or bulk-changing their statuses.

### TDD and review evidence

Each changed behavior was introduced by a failing public CLI fixture test before its minimal implementation. The final suite has 15 tests. Observed red cases included non-ready status admission, qualified dependency confusion, unsupported/partial expressions, cycles, unreviewed legacy work, empty JSON output, duplicate identities/metadata, comment-supplied metadata, canceled prerequisites, conflicting legacy decisions and invalid completed-prerequisite graphs. Existing sentence-ending periods and parenthesized notes are preserved without treating prose numbers as edges.

A read-only review identified note swallowing, duplicate fields, legacy-record placement and transitive missing prerequisites behind done markers. All were reproduced through the CLI and fixed. The script is an eligibility guard over trusted tracker records, not proof of human identity or runtime correctness.

Targeted command: `python3 -m unittest discover -s scripts/tests -p test_frontier.py -v` — 15 tests passed. The suite is now included in the repository gate. `git diff --check` also passed.

### Repository gate

`./scripts/check.sh` passed; full output is pasted below and saved in the feature's `frontier-gate-output.txt`.

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
Ran 15 tests in 2.542s

OK
==> clippy
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.15s
==> test
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.15s
     Running unittests src/lib.rs (target/debug/deps/aahc_core-3656c1a115689065)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/main.rs (target/debug/deps/aahc_core-7e75ddca3e06342d)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/config.rs (target/debug/deps/config-b63ec7559c776cff)

running 5 tests
test database_path_is_read_from_the_environment ... ok
test bind_address_is_configurable ... ok
test bind_address_defaults_to_localhost ... ok
test invalid_bind_address_is_a_typed_error_naming_the_variable ... ok
test missing_database_path_is_a_typed_error_naming_the_variable ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/db_schema.rs (target/debug/deps/db_schema-30b5a109867f54d7)

running 6 tests
test init_creates_every_table_the_spec_requires ... ok
test router_pool_is_seeded_with_the_eleven_free_lanes ... ok
test employee_table_carries_the_spec_columns_and_check_constraints ... ok
test unique_lane_index_prevents_duplicate_lanes ... ok
test seeding_is_idempotent_across_runs ... ok
test schema_matches_the_frozen_python_oracle ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s

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
