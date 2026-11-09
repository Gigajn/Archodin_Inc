#!/usr/bin/env python3
"""Print tickets ready to work now, from .scratch/<feature>/issues/*.md.

Usage:
  frontier.py [--json] [--all]
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

CLOSED = {"done", "wontfix", "cancelled", "canceled"}
OPEN_STATES = {"ready-for-agent", "in-progress", "needs-triage", "needs-info", "ready-for-human"}


def has_legacy_review(text: str) -> bool:
    comments = re.search(r"(?ms)^##[ \t]+Comments[ \t]*\n(.*?)(?=^##[ \t]|\Z)", text)
    if not comments:
        return False
    review = re.search(
        r"(?ms)^###[ \t]+Legacy revalidation[ \t]*\n(.*?)(?=^#{1,3}[ \t]|\Z)",
        comments.group(1),
    )
    if not review:
        return False
    body = review.group(1)
    reviews = re.findall(r"(?m)^\*{0,2}Review:[ \t]*([^\n]*)", body)
    confirmations = re.findall(r"(?m)^\*{0,2}Owner confirmation:[ \t]*([^\n]*)", body)
    return bool(
        len(reviews) == 1 and reviews[0].strip().lstrip("*").strip()
        and len(confirmations) == 1
        and confirmations[0].strip().lstrip("*").strip().lower() == "approved"
    )


def parse_ticket(path: Path) -> dict:
    text = path.read_text(encoding="utf-8", errors="replace")
    metadata = re.split(r"(?m)^##[ \t]+", text, maxsplit=1)[0]
    m = re.match(r"#\s*(\d+):\s*(.+)", text)
    num = m.group(1) if m else path.name.split("-", 1)[0]
    title = m.group(2).strip() if m else path.stem
    statuses = re.findall(r"(?m)^\*{0,2}Status:[ \t]*([^\n]+)", metadata)
    status = (statuses[0].strip().strip("*").strip().lower() if len(statuses) == 1 else "needs-triage")
    blocked: list[str] = []
    dependency_error = "missing Blocked by field"
    blocker_fields = re.findall(r"(?m)^\*{0,2}Blocked by:[ \t]*([^\n]+)", metadata)
    if len(blocker_fields) > 1:
        dependency_error = "ambiguous Blocked by fields"
    if len(blocker_fields) == 1:
        line = blocker_fields[0].strip().lstrip("*").strip().removesuffix(".")
        # A trailing parenthesized note is prose, not an additional dependency.
        note = re.fullmatch(r"(.+?)(?:\s+\([^()]*\)\.?)?", line)
        expression = note.group(1).strip() if note else ""
        reference_pattern = r"(?:[A-Za-z0-9][A-Za-z0-9_-]*#)?[0-9]+"
        if expression.lower() == "none":
            dependency_error = ""
        elif re.fullmatch(rf"{reference_pattern}(?:,\s*{reference_pattern})*", expression):
            dependency_error = ""
            for reference in expression.split(","):
                feature, separator, number = reference.strip().rpartition("#")
                blocked.append(f"{feature}#{int(number):02d}" if separator else f"{int(number):02d}")
            blocked = sorted(set(blocked))
        else:
            dependency_error = "unsupported Blocked by expression"
    if len(statuses) != 1:
        dependency_error = "missing or ambiguous Status field"
    return {
        "num": f"{int(num):02d}" if str(num).isdigit() else str(num),
        "title": title,
        "status": status,
        "blocked": blocked,
        "dependency_error": dependency_error,
        "legacy_revalidated": has_legacy_review(text),
        "path": str(path),
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", action="store_true", help="machine readable")
    ap.add_argument("--all", action="store_true", help="also show blocked and ineligible open work")
    ap.add_argument("root", nargs="?", default=".", help="repo root")
    args = ap.parse_args()

    root = Path(args.root).resolve()
    tickets: list[dict] = []
    for issues in sorted(root.glob(".scratch/*/issues")):
        for p in sorted(issues.glob("*.md")):
            t = parse_ticket(p)
            t["feature"] = issues.parent.name
            tickets.append(t)


    by_num: dict[str, dict] = {}
    duplicates: set[str] = set()
    for t in tickets:
        key = f"{t['feature']}#{t['num']}"
        if key in by_num:
            duplicates.add(key)
        by_num.setdefault(key, t)

    cyclic: set[str] = set()
    visited: set[str] = set()
    active: list[str] = []

    def visit(key: str) -> None:
        if key in active:
            cyclic.update(active[active.index(key):])
            return
        if key in visited or key not in by_num:
            return
        active.append(key)
        ticket = by_num[key]
        for blocker in ticket["blocked"]:
            visit(blocker if "#" in blocker else f"{ticket['feature']}#{blocker}")
        active.pop()
        visited.add(key)

    for key in by_num:
        visit(key)

    invalid: dict[str, bool] = {}

    def has_invalid_dependencies(key: str) -> bool:
        if key in invalid:
            return invalid[key]
        if key not in by_num or key in duplicates or key in cyclic:
            return True
        ticket = by_num[key]
        invalid[key] = bool(ticket["dependency_error"]) or any(
            has_invalid_dependencies(blocker if "#" in blocker else f"{ticket['feature']}#{blocker}")
            for blocker in ticket["blocked"]
        )
        return invalid[key]

    def is_closed(t: dict) -> bool:
        return t["status"] in CLOSED

    def blockers_open(t: dict) -> list[str]:
        if t["feature"] == "aahc" and not is_closed(t) and not t["legacy_revalidated"]:
            return ["legacy revalidation requires owner approval"]
        if f"{t['feature']}#{t['num']}" in duplicates:
            return ["ambiguous ticket identity"]
        if f"{t['feature']}#{t['num']}" in cyclic:
            return ["dependency cycle"]
        if t["dependency_error"]:
            return [f"invalid dependencies: {t['dependency_error']}"]
        bad = []
        for b in t["blocked"]:
            key = b if "#" in b else f"{t['feature']}#{b}"
            other = by_num.get(key)
            if key in duplicates:
                bad.append(f"{b}(ambiguous)")
            elif other is None:
                bad.append(f"{b}(missing)")
            elif has_invalid_dependencies(key):
                bad.append(f"{b}(invalid prerequisites)")
            elif other["status"] != "done":
                bad.append(b)
        return bad

    ready = [t for t in tickets if t["status"] == "ready-for-agent" and not blockers_open(t)]
    ready.sort(key=lambda t: (t["feature"], t["num"]))

    total, done = len(tickets), sum(1 for t in tickets if is_closed(t))
    waiting = [t for t in tickets if not is_closed(t) and blockers_open(t)]
    ineligible = [t for t in tickets if not is_closed(t) and not blockers_open(t)
                  and t["status"] != "ready-for-agent"]

    if args.json:
        print(json.dumps({
            "total": total, "done": done,
            "ready": [{ "feature": t["feature"], **t} for t in ready],
            "waiting": [{ "feature": t["feature"], "num": t["num"],
                          "blocked_by": blockers_open(t)} for t in waiting],
            "ineligible": [{"feature": t["feature"], "num": t["num"],
                            "status": t["status"]} for t in ineligible],
        }, ensure_ascii=False, indent=2))
        return 0

    print(f"FRONTIER {len(ready)} ready / {total} total ({done} done, {len(waiting)} blocked) [{root}]")
    for t in ready:
        print(f"  -> {t['feature']}#{t['num']} [{t['status']}] {t['title']}")
        print(f"     {t['path']}")
    if waiting and args.all:
        print("waiting:")
        for t in waiting:
            print(f"  .. {t['feature']}#{t['num']} [{t['status']}] blocked by {', '.join(blockers_open(t))}")
    if ineligible and args.all:
        print("ineligible:")
        for t in ineligible:
            print(f"  .. {t['feature']}#{t['num']} [{t['status']}] not ready-for-agent")
    if not ready:
        print("frontier empty: no eligible ready-for-agent tickets; use --all to inspect work")
    return 0


if __name__ == "__main__":
    sys.exit(main())
