#!/usr/bin/env python3
"""Parse an IMPLEMENTER_REPORT with header and field-boundary checks."""
from __future__ import annotations

import re

REQUIRED = ["ROUND", "STATUS", "SUGGESTION_RESOLUTIONS", "SELF_REVIEW", "CHANGED_FILES", "SUMMARY"]
STATUSES = {"DONE", "UNVERIFIED", "BLOCKED", "NEEDS_CONTEXT"}

# A header is either `IMPLEMENTER_REPORT:` or a Markdown heading whose first
# words are IMPLEMENTER_REPORT, optionally followed by a title.
HEADER = re.compile(r"^\s*(?:#{1,6}\s*)?IMPLEMENTER[\s_-]+REPORT\b[^\n]*$", re.I)
_FIELD_TEMPLATE = r"^\s*(?:#{1,6}\s*)?(?:\*\*)?{key}(?:\*\*)?\s*[:：]?\s*(.*)$"
_STATUS = re.compile(
    r"^[ \t]*(?:#{1,6}[ \t]*)?(?:\*\*)?STATUS(?:\*\*)?[ \t]*[:：]?[ \t]*\n?[ \t]*(?:\*\*)?"
    r"(DONE|UNVERIFIED|BLOCKED|NEEDS_CONTEXT)(?:\*\*)?[ \t]*[.。]?[ \t]*$",
    re.I | re.M,
)


def parse_report(text: str) -> dict:
    lines = text.splitlines()
    first = next((line for line in lines if line.strip()), "")
    has_header = bool(HEADER.match(first))
    missing = []
    for key in REQUIRED:
        field = re.compile(_FIELD_TEMPLATE.replace("{key}", re.escape(key)), re.I | re.M)
        if not field.search(text):
            missing.append(key)
    status_match = _STATUS.search(text)
    status = status_match.group(1).upper() if status_match else None
    # A status token that is not a standalone value (e.g. DONEISH) must not
    # inherit a valid prefix.
    if status is None:
        loose = re.search(r"\bSTATUS\b\s*[:：]?\s*([A-Z_]+)", text, re.I)
        if loose:
            status = loose.group(1).upper()
    return {
        "present": bool(text.strip()),
        "header": has_header,
        "pass": has_header and not missing and status in STATUSES,
        "missing": missing,
        "status": status if status in STATUSES else None,
        "raw_status": status,
    }
