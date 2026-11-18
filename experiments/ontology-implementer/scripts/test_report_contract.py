#!/usr/bin/env python3
"""Unit checks for the implementer report parser."""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from report_contract import parse_report  # noqa: E402

BASE = """IMPLEMENTER_REPORT:
ROUND: 0
STATUS: DONE
SUGGESTION_RESOLUTIONS:
none
SELF_REVIEW:
- checked
CHANGED_FILES:
- a.rs
SUMMARY:
done
"""


class ParseReportTest(unittest.TestCase):
    def test_valid(self):
        self.assertTrue(parse_report(BASE)["pass"])

    def test_status_in_fence_cannot_override_formal_error(self):
        text = BASE.replace("STATUS: DONE", "STATUS: ERROR") + "```\nSTATUS: DONE\n```\n"
        parsed = parse_report(text)
        self.assertFalse(parsed["pass"])
        self.assertIsNone(parsed["status"])

    def test_duplicate_statuses_fail(self):
        text = BASE.replace("STATUS: DONE", "STATUS: ERROR") + "STATUS: DONE\n"
        parsed = parse_report(text)
        self.assertFalse(parsed["pass"])
        self.assertTrue(parsed["duplicate_status"])

    def test_status_suffix_is_not_done(self):
        text = BASE.replace("STATUS: DONE", "STATUS: DONE pending tests")
        self.assertFalse(parse_report(text)["pass"])

    def test_prefix_does_not_satisfy_self_review(self):
        text = BASE.replace("SELF_REVIEW:", "SELF_REVIEWER:")
        self.assertFalse(parse_report(text)["pass"])

    def test_heading_status(self):
        text = BASE.replace("STATUS: DONE", "## STATUS\nDONE.")
        self.assertTrue(parse_report(text)["pass"])


if __name__ == "__main__":
    unittest.main()
