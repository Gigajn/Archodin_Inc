# 01: Align Archodin and Basileia sources of truth

**What to build:** A coherent documentation baseline and work-selection policy reflecting the owner's Milestone 0 decisions, without implementing the runtime or migrating existing code.

**Blocked by:** None (explicitly requested by the owner).

**Status:** done

**Spec:** `.scratch/basileia-milestone-0/spec.md`

- [x] Record canonical product names and accepted library/application boundaries.
- [x] Record licensing intent and owner-managed future repository separation.
- [x] Reconcile Goals with the accepted scope and remove mandatory langchain-rust assumptions.
- [x] Preserve the old AAHC spec with an explicit reconciliation notice.
- [x] Define document ownership and require legacy-ticket revalidation before implementation.
- [x] Record documentation validation and repository gate output below.
- [x] Obtain final owner confirmation of shared understanding before marking done.

## Comments

### Owner-confirmed decisions

Q1–Q15 were resolved during the Milestone 0 interview. The owning documents are linked in the spec; implementation details not covered by those decisions remain proposals. No legacy ticket status, remote, license metadata or runtime code was changed by this milestone.

### Validation

`git diff --check` passed. Targeted documentation whitespace and reference-target checks passed (`Documentation checks: PASS`). `./scripts/check.sh` passed on the current workspace: formatting, Clippy with warnings denied, and all 14 existing integration tests. These results validate the current workspace, not an implemented Archodin/Basileia runtime. The owner confirmed Q16 and authorized closing Milestone 0. The repository gate was rerun successfully before closure; the latest raw output is saved alongside the milestone spec. No runtime changes were made.

Full gate output (also saved in `.scratch/basileia-milestone-0/gate-output.txt`):

```text
==> fmt
==> clippy
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.34s
==> test
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.24s
     Running unittests src/lib.rs (target/debug/deps/aahc_core-3656c1a115689065)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/main.rs (target/debug/deps/aahc_core-7e75ddca3e06342d)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/config.rs (target/debug/deps/config-b63ec7559c776cff)

running 5 tests
test bind_address_defaults_to_localhost ... ok
test bind_address_is_configurable ... ok
test database_path_is_read_from_the_environment ... ok
test invalid_bind_address_is_a_typed_error_naming_the_variable ... ok
test missing_database_path_is_a_typed_error_naming_the_variable ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/db_schema.rs (target/debug/deps/db_schema-30b5a109867f54d7)

running 6 tests
test employee_table_carries_the_spec_columns_and_check_constraints ... ok
test init_creates_every_table_the_spec_requires ... ok
test router_pool_is_seeded_with_the_eleven_free_lanes ... ok
test unique_lane_index_prevents_duplicate_lanes ... ok
test seeding_is_idempotent_across_runs ... ok
test schema_matches_the_frozen_python_oracle ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s

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
