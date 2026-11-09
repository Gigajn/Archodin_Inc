"""Behavioral tests at the frontier CLI, using isolated tracker fixtures."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "frontier.py"


class FrontierCliTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)

    def ticket(self, number, *, feature="project", status="ready-for-agent", blockers="None", extra=""):
        path = self.root / ".scratch" / feature / "issues" / f"{number:02d}-task.md"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            f"# {number:02d}: Task {number}\n\n"
            f"**Status:** {status}\n\n"
            f"**Blocked by:** {blockers}\n{extra}",
            encoding="utf-8",
        )
        return path

    def run_cli(self, *options):
        result = subprocess.run(
            [sys.executable, str(SCRIPT), *options, str(self.root)],
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def report(self):
        return json.loads(self.run_cli("--json"))

    def test_only_ready_for_agent_tickets_are_offered(self):
        for number, status in enumerate(
            ["ready-for-agent", "needs-triage", "needs-info", "ready-for-human", "in-progress"],
            start=1,
        ):
            self.ticket(number, status=status)

        self.assertEqual([ticket["num"] for ticket in self.report()["ready"]], ["01"])

    def test_qualified_blocker_uses_the_named_feature(self):
        self.ticket(1, status="done")
        self.ticket(1, feature="other")
        self.ticket(2, blockers="other#01")

        self.assertEqual(
            [(ticket["feature"], ticket["num"]) for ticket in self.report()["ready"]],
            [("other", "01")],
        )
        self.ticket(1, feature="other", status="done")
        self.assertEqual([ticket["num"] for ticket in self.report()["ready"]], ["02"])

    def test_unsupported_dependency_input_fails_closed(self):
        self.ticket(1, status="done")
        for blockers in ["unknown", "01, foreign", "None, 01", "01 or 02", "other#", "01,", "01 (done), other#01 (pending)"]:
            with self.subTest(blockers=blockers):
                self.ticket(2, blockers=blockers)
                report = self.report()
                self.assertEqual(report["ready"], [])
                self.assertIn("invalid", " ".join(report["waiting"][0]["blocked_by"]))

    def test_dependency_cycles_are_reported_as_blocked(self):
        self.ticket(1, blockers="other#01")
        self.ticket(1, feature="other", blockers="project#01")

        report = self.report()
        self.assertEqual(report["ready"], [])
        self.assertEqual(len(report["waiting"]), 2)
        for ticket in report["waiting"]:
            self.assertIn("cycle", " ".join(ticket["blocked_by"]))

    def test_legacy_work_requires_explicit_owner_revalidation(self):
        path = self.ticket(1, feature="aahc")
        original = path.read_text()
        report = self.report()
        self.assertEqual(report["ready"], [])
        self.assertIn("revalidation", " ".join(report["waiting"][0]["blocked_by"]))
        self.assertEqual(path.read_text(), original)

        self.ticket(1, feature="aahc", extra="\n**Legacy revalidation:** owner-approved\n")
        self.assertEqual(self.report()["ready"], [])
        self.ticket(
            1,
            feature="aahc",
            extra="\n## Comments\n\n### Legacy revalidation\n\n"
            "**Review:** Still relevant after checking the accepted product scope.\n"
            "**Owner confirmation:** approved\n",
        )
        self.assertEqual([ticket["num"] for ticket in self.report()["ready"]], ["01"])

    def test_empty_tracker_has_consistent_json_and_human_output(self):
        report = self.report()
        self.assertEqual(report["total"], 0)
        self.assertEqual(report["ready"], [])
        self.assertEqual(report["waiting"], [])
        self.assertIn("FRONTIER 0 ready / 0 total", self.run_cli())

    def test_duplicate_prerequisite_identity_is_not_treated_as_done(self):
        path = self.ticket(1, status="done")
        path.with_name("01-duplicate.md").write_text(path.read_text(), encoding="utf-8")
        self.ticket(2, blockers="01")

        report = self.report()
        self.assertEqual(report["ready"], [])
        self.assertIn("ambiguous", " ".join(report["waiting"][0]["blocked_by"]))

    def test_comment_examples_cannot_supply_missing_metadata(self):
        path = self.ticket(1)
        path.write_text(
            "# 01: Missing metadata\n\n**Status:** ready-for-agent\n\n"
            "## Comments\n\nExample only:\n**Blocked by:** None\n",
            encoding="utf-8",
        )

        report = self.report()
        self.assertEqual(report["ready"], [])
        self.assertIn("invalid", " ".join(report["waiting"][0]["blocked_by"]))

    def test_parenthesized_notes_do_not_create_dependencies(self):
        self.ticket(1, blockers="None (approved by owner).")
        self.ticket(2, blockers="01 (implies 07 and 99).")

        report = self.report()
        self.assertEqual([ticket["num"] for ticket in report["ready"]], ["01"])
        self.assertEqual(report["waiting"][0]["blocked_by"], ["01"])

    def test_closed_without_completion_does_not_satisfy_a_prerequisite(self):
        for status in ["wontfix", "cancelled", "canceled"]:
            with self.subTest(status=status):
                self.ticket(1, status=status)
                self.ticket(2, blockers="01")
                self.assertEqual(self.report()["ready"], [])

    def test_duplicate_metadata_fields_fail_closed(self):
        for extra in ["**Blocked by:** other#99", "**Status:** in-progress"]:
            with self.subTest(extra=extra):
                self.ticket(1, extra="\n" + extra + "\n")
                report = self.report()
                self.assertEqual(report["ready"], [])
                self.assertIn("invalid", " ".join(report["waiting"][0]["blocked_by"]))

    def test_human_and_json_reports_explain_the_same_eligibility(self):
        self.ticket(1)
        self.ticket(2, status="needs-info")
        self.ticket(3, blockers="missing#01")

        report = self.report()
        self.assertEqual([ticket["num"] for ticket in report["ready"]], ["01"])
        self.assertEqual([ticket["num"] for ticket in report["ineligible"]], ["02"])
        self.assertIn("missing#01(missing)", report["waiting"][0]["blocked_by"])
        human = self.run_cli("--all")
        offered = [line.split()[1] for line in human.splitlines() if line.startswith("  -> ")]
        self.assertEqual(offered, ["project#01"])
        self.assertIn("needs-info", human)
        self.assertIn("missing#01(missing)", human)

    def test_empty_or_conflicting_legacy_review_is_not_approval(self):
        for review in [
            "**Review:**\n**Owner confirmation:** approved\n",
            "**Review:** Checked scope.\n**Owner confirmation:** approved\n"
            "**Owner confirmation:** denied\n",
        ]:
            with self.subTest(review=review):
                self.ticket(
                    1, feature="aahc",
                    extra="\n## Comments\n\n### Legacy revalidation\n\n" + review,
                )
                self.assertEqual(self.report()["ready"], [])

    def test_done_marker_cannot_hide_invalid_or_cyclic_metadata(self):
        self.ticket(5, status="done", blockers="missing#99")
        for blockers in ["unknown", "01", "missing#99", "05"]:
            with self.subTest(blockers=blockers):
                self.ticket(1, status="done", blockers=blockers)
                self.ticket(2, blockers="01")
                self.assertEqual(self.report()["ready"], [])

    def test_sentence_terminated_dependency_is_parsed(self):
        self.ticket(1, status="done")
        self.ticket(2, blockers="01.")

        self.assertEqual([ticket["num"] for ticket in self.report()["ready"]], ["02"])


if __name__ == "__main__":
    unittest.main()
