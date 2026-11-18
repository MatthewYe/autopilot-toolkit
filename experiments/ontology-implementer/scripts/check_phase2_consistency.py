#!/usr/bin/env python3
"""Check that a phase-2 JSON and its saved diff describe the same run."""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


def diff_paths(diff: str) -> list[str]:
    return [m.group(2) for m in re.finditer(r"^diff --git a/(\S+) b/(\S+)$", diff, re.M)]


def report_from_diff(diff: str) -> str | None:
    match = re.search(
        r"^diff --git a/IMPLEMENTER_REPORT\.md b/IMPLEMENTER_REPORT\.md\n(.*?)(?=^diff --git |\Z)",
        diff,
        re.M | re.S,
    )
    if not match:
        return None
    lines = []
    for line in match.group(1).splitlines():
        if line.startswith("+") and not line.startswith("+++"):
            lines.append(line[1:])
    return "\n".join(lines).strip() + "\n"


def check(result_path: Path) -> list[str]:
    data = json.loads(result_path.read_text())
    diff_path = result_path.parent / data["agent_diff"]
    if not diff_path.exists():
        return [f"missing diff: {diff_path}"]
    diff = diff_path.read_text()
    paths = diff_paths(diff)
    expected = sorted(data.get("changed_files", []))
    actual = sorted(p for p in paths if p != "IMPLEMENTER_REPORT.md")
    errors = []
    if actual != expected:
        errors.append(f"changed_files mismatch: json={expected} diff={actual}")
    if not data.get("report_text", "").strip():
        errors.append("json has no report_text")
    else:
        in_diff = report_from_diff(diff)
        if in_diff != data["report_text"].strip() + "\n":
            errors.append("report_text does not match the diff")
    return errors


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("paths", nargs="+")
    args = ap.parse_args()
    failed = False
    for raw in args.paths:
        p = Path(raw)
        errors = check(p)
        if errors:
            failed = True
            print(f"FAIL {p}")
            for e in errors:
                print(f"  {e}")
        else:
            print(f"OK   {p}")
    raise SystemExit(1 if failed else 0)


if __name__ == "__main__":
    main()
