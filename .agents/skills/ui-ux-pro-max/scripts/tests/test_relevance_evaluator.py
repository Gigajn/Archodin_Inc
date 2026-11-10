#!/usr/bin/env python3
"""Relevance evaluator tests — skipped.

The module under test, scripts/evaluate-relevance.py, is no longer in the
repo, so every test in this module imported a missing script (and module
import itself failed while locating it). Per the skill-tree tidy decision
(Q9, option b) the tests are skipped rather than restoring the missing
script. If the evaluator returns, restore the tests from git history.
"""

import unittest


class RelevanceEvaluatorTest(unittest.TestCase):
    @unittest.skip(
        "evaluate-relevance.py is absent; all relevance-evaluator tests "
        "imported the missing script"
    )
    def test_relevance_evaluator(self):
        pass


if __name__ == "__main__":
    unittest.main(verbosity=2)
