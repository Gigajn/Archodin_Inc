# Rust conventions — S-01 review workspace

Candidate only: the installed `rust-conventions` is unchanged and matches `skill-snapshot`. Do not accept or install the candidate before actual owner review. S-01 remains in-progress and S-02 remains blocked.

## Review

Open `iteration-1/review.html` in a browser. Outputs shows the six design answers and their grading; Benchmark compares the candidate (`with_skill`) against the old version (`without_skill` in the benchmark, `old_skill` in directories). Submit All Reviews downloads `feedback.json`; return it for the next iteration, or give explicit feedback in conversation. No review approval has been recorded.

- `candidate/SKILL.md` and `candidate/legacy-application.md`: proposed scoped guidance.
- `skill-snapshot/SKILL.md`: preserved baseline.
- `candidate/evals/evals.json`: approved scenario prompts and design assertions.
- `iteration-1/benchmark.md`: results and limitations.
- `iteration-1/eval-*/`: output, action transcript, grading and unknown timing for every run.
- `build_report.py`: repeatable aggregation and static report generation.
- `tooling/sources.json`: downloaded upstream tool URLs and SHA-256 hashes. Sources were inspected; only the official generator's static branch was executed. The upstream aggregator was not executed because its missing-metric defaults would misrepresent unavailable data.

These are model-generated design answers, not compiled Provider/Tool tests. There was one run per scenario/configuration; no reliability, speed or cost improvement is established. Unknown model token/time metrics remain unknown.

The generated viewer has external font and unused XLSX CDN tags removed for offline use. No server was started and no project code or credentials were uploaded. The workspace is ignored by Git; an evidence snapshot is copied to the compatibility-spike tracker directory for preservation. That copy is an archive, not a second installed Skill or an instruction to execute legacy tickets.
