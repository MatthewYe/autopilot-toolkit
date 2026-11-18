#!/usr/bin/env python3
"""Parse an IMPLEMENTER_REPORT with header and field-boundary checks."""
from __future__ import annotations

import re

REQUIRED = ["ROUND", "STATUS", "SUGGESTION_RESOLUTIONS", "SELF_REVIEW", "CHANGED_FILES", "SUMMARY"]
STATUSES = {"DONE", "UNVERIFIED", "BLOCKED", "NEEDS_CONTEXT"}

# A header is either `IMPLEMENTER_REPORT:` or a Markdown heading whose first
# words are IMPLEMENTER_REPORT, optionally followed by a title.
HEADER = re.compile(r"^\s*(?:#{1,6}\s*)?IMPLEMENTER[\s_-]+REPORT\b[^\n]*$", re.I)
# A field is either `KEY: value` (inline) or a Markdown heading whose line is
# exactly KEY (value follows on later lines).  A prefix such as SELF_REVIEWER
# must not satisfy SELF_REVIEW.
_INLINE = r"^\s*(?:#{1,6}\s*)?(?:\*\*)?{key}(?:\*\*)?\s*[:：]\s*(.*)$"
_HEADING = r"^\s*#{1,6}\s*(?:\*\*)?{key}(?:\*\*)?\s*$"
_STATUS = re.compile(
    r"^[ \t]*(?:#{1,6}[ \t]*)?(?:\*\*)?STATUS(?:\*\*)?[ \t]*[:：]?[ \t]*"
    r"(?:\n[ \t]*){0,4}(?:\*\*)?"
    r"(DONE|UNVERIFIED|BLOCKED|NEEDS_CONTEXT)(?:\*\*)?[ \t]*[.。]?[ \t]*$",
    re.I | re.M,
)


def parse_report(text: str) -> dict:
    lines = text.splitlines()
    first = next((line for line in lines if line.strip()), "")
    has_header = bool(HEADER.match(first))
    missing = []
    for key in REQUIRED:
        esc = re.escape(key)
        inline = re.compile(_INLINE.replace("{key}", esc), re.I | re.M)
        heading = re.compile(_HEADING.replace("{key}", esc), re.I | re.M)
        if not (inline.search(text) or heading.search(text)):
            missing.append(key)
    status_match = _STATUS.search(text)
    strict_status = status_match.group(1).upper() if status_match else None
    # Loose status is diagnostic only; it never decides pass.  A value such as
    # `DONE pending tests` must fail even though it starts with DONE.
    loose_match = re.search(r"\bSTATUS\b\s*[:：]?\s*([A-Z_]+)", text, re.I)
    loose_status = loose_match.group(1).upper() if loose_match else None
    return {
        "present": bool(text.strip()),
        "header": has_header,
        "pass": has_header and not missing and strict_status in STATUSES,
        "missing": missing,
        "status": strict_status if strict_status in STATUSES else None,
        "raw_status": loose_status,
        "strict_status": strict_status,
    }
