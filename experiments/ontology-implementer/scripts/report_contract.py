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


def _visible_lines(text: str) -> list[str]:
    """Return lines outside fenced code blocks.

    A code sample must not be able to supply a formal field or a formal status.
    """
    out: list[str] = []
    in_fence = False
    for line in text.splitlines():
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            continue
        if not in_fence:
            out.append(line)
    return out


def _status_values(lines: list[str]) -> list[str]:
    """Collect bare STATUS declarations, not examples inside prose/code."""
    inline = re.compile(
        r"^\s*(?:#{1,6}\s*)?(?:\*\*)?STATUS(?:\*\*)?\s*[:：]\s*(.*?)\s*$",
        re.I,
    )
    heading = re.compile(r"^\s*#{1,6}\s*(?:\*\*)?STATUS(?:\*\*)?\s*$", re.I)
    values: list[str] = []
    for i, line in enumerate(lines):
        match = inline.match(line)
        if match:
            values.append(match.group(1).strip())
            continue
        if not heading.match(line):
            continue
        j = i + 1
        while j < len(lines) and not lines[j].strip():
            j += 1
        if j < len(lines):
            values.append(lines[j].strip())
    return values


def parse_report(text: str) -> dict:
    lines = _visible_lines(text)
    visible = "\n".join(lines)
    first = next((line for line in lines if line.strip()), "")
    has_header = bool(HEADER.match(first))
    missing = []
    for key in REQUIRED:
        esc = re.escape(key)
        inline = re.compile(_INLINE.replace("{key}", esc), re.I | re.M)
        heading = re.compile(_HEADING.replace("{key}", esc), re.I | re.M)
        if not (inline.search(visible) or heading.search(visible)):
            missing.append(key)

    status_values = _status_values(lines)
    duplicate_status = len(status_values) > 1
    candidate = status_values[0] if len(status_values) == 1 else None
    strict_status = None
    if candidate is not None:
        cleaned = candidate.strip().strip("*").strip().rstrip(".。").strip().upper()
        if cleaned in STATUSES:
            strict_status = cleaned
    loose_match = re.search(r"\bSTATUS\b\s*[:：]?\s*([A-Z_]+)", visible, re.I)
    loose_status = loose_match.group(1).upper() if loose_match else None
    return {
        "present": bool(text.strip()),
        "header": has_header,
        "pass": has_header and not missing and strict_status in STATUSES and not duplicate_status,
        "missing": missing,
        "status": strict_status,
        "raw_status": loose_status,
        "strict_status": strict_status,
        "duplicate_status": duplicate_status,
        "status_values": status_values,
    }
