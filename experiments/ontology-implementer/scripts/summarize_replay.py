#!/usr/bin/env python3
"""Summarize Phase 2 replay JSON files into a small table."""
from __future__ import annotations

import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DIR = ROOT / "results" / "phase2"
REQUIRED = ["ROUND", "STATUS", "SUGGESTION_RESOLUTIONS", "SELF_REVIEW", "CHANGED_FILES", "SUMMARY"]
STATUSES = {"DONE", "UNVERIFIED", "BLOCKED", "NEEDS_CONTEXT"}


def report_contract(repo: Path):
    p = repo / "IMPLEMENTER_REPORT.md"
    if not p.exists():
        return {"present": False, "pass": False, "missing": REQUIRED}
    text = p.read_text()
    missing = [key for key in REQUIRED if not re.search(rf"\b{key}\b", text)]
    status = re.search(r"\bSTATUS\b\s*[:：]?\s*`?(DONE|UNVERIFIED|BLOCKED|NEEDS_CONTEXT)", text)
    return {
        "present": True,
        "pass": not missing and bool(status),
        "missing": missing,
        "status": status.group(1) if status else None,
    }


def main():
    rows = []
    for p in sorted(DIR.glob("*.json")):
        if p.name in {"summary.json", "gold.json"}:
            continue
        d = json.loads(p.read_text())
        repo = Path(d["repo"])
        contract = report_contract(repo)
        structural = subprocess.run(
            ["python3", str(ROOT / "scripts/check_t126_contract.py"), str(repo)],
            text=True, capture_output=True,
        )
        structural_pass = structural.returncode == 0
        checks = d.get("checks", {})
        parallel = checks.get("workspace_tests", {}).get("exit")
        serial = d.get("serial_rerun", {}).get("exit")
        row = {
            "run": p.stem,
            "workspace_tests": parallel,
            "workspace_tests_serial": serial,
            "toolkit_setup": checks.get("toolkit_setup_tests", {}).get("exit"),
            "skill_check": checks.get("skill_check", {}).get("exit"),
            "validation": checks.get("validation", {}).get("exit"),
            "clippy": checks.get("clippy", {}).get("exit"),
            "structural": structural_pass,
            "report_contract": contract["pass"],
            "report_status": contract.get("status"),
            "changed_files": len(d.get("changed_files", [])),
            "hidden_test_pass": (serial == 0) if serial is not None else parallel == 0,
        }
        row["full_pass"] = bool(row["hidden_test_pass"] and structural_pass and contract["pass"])
        rows.append(row)
    (DIR / "summary.json").write_text(json.dumps(rows, indent=2))
    print("| run | hidden | hidden(serial) | structural | report | status | files |")
    print("|---|---|---|---|---|---|---|")
    for r in rows:
        print(f"| {r['run']} | {r['workspace_tests']} | {r['workspace_tests_serial']} | "
              f"{r['structural']} | {r['report_contract']} | {r['report_status']} | {r['changed_files']} |")


if __name__ == "__main__":
    main()
