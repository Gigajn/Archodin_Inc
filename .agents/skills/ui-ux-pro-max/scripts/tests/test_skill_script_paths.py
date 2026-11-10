#!/usr/bin/env python3
"""Skill script paths tests — skipped.

The repo-root anchor this test used, scripts/generate-catalog-summary.py,
is no longer in the repo, so module import itself failed (StopIteration)
while locating the repository root. The test also assumed the upstream
plugin layout (cli/assets/skills), which this repo does not ship. Per the
skill-tree tidy decision (Q9, option b, extended to this file) the tests
are skipped rather than restoring the missing script. If
generate-catalog-summary.py returns, restore the tests from git history.
"""

import unittest


class SkillScriptPathsTest(unittest.TestCase):
    @unittest.skip(
        "scripts/generate-catalog-summary.py is absent; "
        "the repo-root anchor cannot be located"
    )
    def test_every_shipped_markdown_invocation_resolves_from_the_skill_directory(self):
        pass


if __name__ == "__main__":
    unittest.main()
