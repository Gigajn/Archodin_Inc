#!/usr/bin/env python3
"""The catalog snapshot must not depend on the checkout's line endings.

Regression test for bd19ab9 (#462), where catalog-summary.json was regenerated
on a CRLF checkout. Every recorded sha256 was the CRLF hash of the source file,
so `verify:data` failed on every LF platform, including CI.
"""

import hashlib
import json
import shutil
import unittest
from pathlib import Path

REPO = next(
    parent for parent in Path(__file__).resolve().parents
    if (parent / "data" / "catalog-summary.json").is_file()
)
DATA = REPO / "data"
SNAPSHOT_FILES = (
    "google-fonts.csv",
    "google-font-licenses.json",
    "icons.csv",
    "phosphor-icons-upstream.json",
)


# NOTE: test_digest_is_identical_for_lf_and_crlf and
# test_crlf_checkout_produces_the_committed_hashes were removed: they tested
# digest() from scripts/generate-catalog-summary.py, which is no longer in
# the repo (Q9 option b: fix the tests, do not restore the missing script).
# The surviving test below still guards the bd19ab9 regression using only
# the committed snapshot and data files.


class CatalogSummaryLineEndingsTest(unittest.TestCase):
    def test_committed_snapshot_matches_normalized_sources(self):
        summary = json.loads((DATA / "catalog-summary.json").read_text(encoding="utf-8"))
        for name in SNAPSHOT_FILES:
            expected = hashlib.sha256(
                (DATA / name).read_bytes().replace(b"\r\n", b"\n")
            ).hexdigest()
            self.assertEqual(
                summary["snapshots"][name]["sha256"], expected,
                f"{name}: committed snapshot hash does not match the LF-normalized source",
            )


if __name__ == "__main__":
    unittest.main()
