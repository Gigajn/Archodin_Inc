"""Build the S-01 guidance report without fabricating unavailable usage metrics."""
import json
import re
import statistics
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent
ITERATION = ROOT / "iteration-1"
CONFIGS = {"with_skill": "with_skill", "without_skill": "old_skill"}


def stats(values):
    return {
        "mean": statistics.mean(values),
        "stddev": statistics.stdev(values) if len(values) > 1 else 0,
        "min": min(values),
        "max": max(values),
    }


runs = []
for configuration, directory in CONFIGS.items():
    for case in sorted(ITERATION.glob("eval-*")):
        metadata = json.loads((case / "eval_metadata.json").read_text())
        grading = json.loads((case / directory / "grading.json").read_text())
        expectations = grading["expectations"]
        assert [e["text"] for e in expectations] == metadata["assertions"]
        assert all(isinstance(e["passed"], bool) and e["evidence"] for e in expectations)
        passed = sum(e["passed"] for e in expectations)
        total = len(expectations)
        result = dict(passed=passed, failed=total - passed, total=total,
                      pass_rate=passed / total, time_seconds=None, tokens=None,
                      tool_calls=None, errors=None)
        runs.append(dict(eval_id=metadata["eval_id"], eval_name=metadata["eval_name"],
                         configuration=configuration, run_number=1, result=result,
                         expectations=expectations,
                         notes=["Design/guidance evaluation only; no compiled runtime evidence."]))

summary = {}
for config in CONFIGS:
    values = [r["result"]["pass_rate"] for r in runs if r["configuration"] == config]
    summary[config] = dict(pass_rate=stats(values), time_seconds=None, tokens=None)
summary["delta"] = dict(
    pass_rate=f'{summary["with_skill"]["pass_rate"]["mean"] - summary["without_skill"]["pass_rate"]["mean"]:+.4f}',
    time_seconds=None, tokens=None,
)
notes = [
    "with_skill = candidate; without_skill = preserved old_skill baseline. Active skill remains unchanged pending owner review.",
    "One run per scenario/configuration; dispersion across three different scenarios is not repeatability evidence or statistical significance.",
    "Candidate passes 13/13 assertions; baseline passes 12/13. Provider and legacy cases tie. The Tool baseline does not specify validation before effects.",
    "Most assertions pass in both configurations and do not discriminate their quality in these samples. Legacy parity is a proposed characterization plan, not verified runtime parity.",
    "The baseline Tool plan also leaves domain-code relevance unresolved and offers bounded wall-clock fallback; these are qualitative limitations, not extra scored failures.",
    "Token usage, run durations and full execution tool counts are unavailable from Zed spawn_agent; unknown fields remain null and time/token aggregates are omitted from display.",
    "Manual schema-compatible aggregation preserves unknown values. The downloaded upstream aggregator expects run-* subdirectories and substitutes zero/output characters for missing metrics, so it was inspected but not executed.",
    "The viewer uses the official generator. External font links and the unused XLSX CDN script are removed from its generated HTML for offline review; upstream source files remain intact.",
]
benchmark = dict(
    metadata=dict(skill_name="rust-conventions", skill_path=str(ROOT / "candidate"),
                  executor_model="unknown (configured Zed subagent default)",
                  analyzer_model="unknown (configured Zed subagent default)",
                  timestamp=datetime.now(timezone.utc).isoformat(),
                  evals_run=[1, 2, 3], runs_per_configuration=1),
    runs=runs, run_summary=summary, notes=notes,
)
(ITERATION / "benchmark.json").write_text(json.dumps(benchmark, indent=2) + "\n")
lines = ["# Rust guidance evaluation — iteration 1", "", "Candidate pending owner review.", "",
         "| Scenario | Candidate | Baseline |", "|---|---:|---:|"]
for candidate, baseline in zip(runs[:3], runs[3:]):
    a, b = candidate["result"], baseline["result"]
    lines.append(f'| {candidate["eval_name"]} | {a["passed"]}/{a["total"]} | {b["passed"]}/{b["total"]} |')
lines.extend(["", "## Interpretation and limitations", ""] + [f"- {n}" for n in notes])
(ITERATION / "benchmark.md").write_text("\n".join(lines) + "\n")
viewer = ITERATION / "review.html"
subprocess.run([sys.executable, str(ROOT / "tooling/eval-viewer/generate_review.py"),
                str(ITERATION), "--skill-name", "rust-conventions", "--benchmark",
                str(ITERATION / "benchmark.json"), "--static", str(viewer)],
               check=True, timeout=30)
html = viewer.read_text()
html = re.sub(r'^\s*<link\b[^>]*https://[^>]*>\s*$', '', html, flags=re.MULTILINE)
html = re.sub(r'^\s*<script\b[^>]*src="https://[^>]*></script>\s*$', '', html, flags=re.MULTILINE)
assert not re.search(r'<(?:script|link)\b[^>]*(?:src|href)="https?://', html)
assert len(runs) == 6
assert 'const EMBEDDED_DATA = ' in html
viewer.write_text(html)
print("PASS: six graded outputs embedded; unknown metrics preserved; offline review generated.")
