# Rust guidance evaluation — iteration 1

Candidate pending owner review.

| Scenario | Candidate | Baseline |
|---|---:|---:|
| generic-provider | 4/4 | 4/4 |
| typed-tool-async | 5/5 | 4/5 |
| legacy-router | 4/4 | 4/4 |

## Interpretation and limitations

- with_skill = candidate; without_skill = preserved old_skill baseline. Active skill remains unchanged pending owner review.
- One run per scenario/configuration; dispersion across three different scenarios is not repeatability evidence or statistical significance.
- Candidate passes 13/13 assertions; baseline passes 12/13. Provider and legacy cases tie. The Tool baseline does not specify validation before effects.
- Most assertions pass in both configurations and do not discriminate their quality in these samples. Legacy parity is a proposed characterization plan, not verified runtime parity.
- The baseline Tool plan also leaves domain-code relevance unresolved and offers bounded wall-clock fallback; these are qualitative limitations, not extra scored failures.
- Token usage, run durations and full execution tool counts are unavailable from Zed spawn_agent; unknown fields remain null and time/token aggregates are omitted from display.
- Manual schema-compatible aggregation preserves unknown values. The downloaded upstream aggregator expects run-* subdirectories and substitutes zero/output characters for missing metrics, so it was inspected but not executed.
- The viewer uses the official generator. External font links and the unused XLSX CDN script are removed from its generated HTML for offline review; upstream source files remain intact.
