#!/usr/bin/env python3
"""Catalog refresh tests — skipped.

The scripts under test, refresh-google-fonts.py and refresh-icon-catalog.py,
are no longer in the repo, so every test in this module invoked a missing
script (and module import itself failed while locating them). Per the
skill-tree tidy decision (Q9, option b) the tests are skipped rather than
restoring the missing scripts. If the refresh scripts return, restore the
tests from git history.
"""

import unittest


class CatalogRefreshTest(unittest.TestCase):
    @unittest.skip(
        "refresh-google-fonts.py and refresh-icon-catalog.py are absent; "
        "all catalog-refresh tests invoked the missing scripts"
    )
    def test_catalog_refresh(self):
        pass


if __name__ == "__main__":
    unittest.main()
