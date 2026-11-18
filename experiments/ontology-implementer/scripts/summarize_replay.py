#!/usr/bin/env python3
"""Summarize Phase 2 replay JSON files into a small table.

The summary is derived only from persisted result JSON; it does not inspect a
worktree, run a command, or depend on an absolute path.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DIR = ROOT / "results" / "phase2"

sys.path.insert(0, str(ROOT / "scripts"))
from report_contract import parse_report  # noqa: E402


def main() -> None:
    import argparse
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", default=str(DIR))
    args = ap.parse_args()
    directory = Path(args.dir)
    rows = []
    for p in sorted(directory.glob("*.json")):
        if p.name in {"summary.json", "gold.json"} or p.name.endswith("-integration.json") or p.name == "tokens.json":
            continue
        d = json.loads(p.read_text())
        checks = d.get("checks", {})
        ws = checks.get("workspace_tests", {})
        is_serial = "--test-threads=1" in ws.get("cmd", "")
        serial = ws.get("exit") if is_serial else None
        parallel = None if is_serial else ws.get("exit")
        report = d.get("report") or parse_report(d.get("report_text", ""))
        structural = d.get("structural", {}).get("pass")
        if structural is None:
            structural = bool(d.get("structural_pass"))
        row = {
            "run": d.get("run") or p.stem,
            "workspace_tests": parallel,
            "workspace_tests_serial": serial,
            "toolkit_setup": checks.get("toolkit_setup_tests", {}).get("exit"),
            "skill_check": checks.get("skill_check", {}).get("exit"),
            "validation": checks.get("validation", {}).get("exit"),
            "clippy": checks.get("clippy", {}).get("exit"),
            "structural": bool(structural),
            "report_contract": bool(report.get("pass")),
            "report_status": report.get("status"),
            "changed_files": len(d.get("changed_files", [])),
            "hidden_test_pass": (serial == 0) if serial is not None else parallel == 0,
        }
        required = ["toolkit_setup", "skill_check", "validation", "clippy"]
        # Presence is whether the check recorded an exit code.  A non-zero
        # exit is an executed failure, not a missing record, and must stay
        # distinguishable in the summary.
        ws_present = isinstance(ws, dict) and "exit" in ws
        ws_exit = ws.get("exit") if ws_present else None
        ws_ok = ws_exit == 0
        checks_ok = ws_ok and all(row.get(k) == 0 for k in required)
        row["checks_ok"] = checks_ok
        row["checks_missing"] = ([k for k in required if row.get(k) is None]
                                 + ([] if ws_present else ["workspace_tests"]))
        row["checks_failed"] = ([k for k in required if row.get(k) not in (None, 0)]
                                + ([] if not ws_present or ws_exit == 0 else ["workspace_tests"]))
        row["full_pass"] = bool(
            row["hidden_test_pass"] and row["structural"] and row["report_contract"] and checks_ok
        )
        rows.append(row)
    (directory / "summary.json").write_text(json.dumps(rows, indent=2))
    print("| run | hidden | hidden(serial) | structural | report | status | files |")
    print("|---|---|---|---|---|---|---|")
    for r in rows:
        print(f"| {r['run']} | {r['workspace_tests']} | {r['workspace_tests_serial']} | "
              f"{r['structural']} | {r['report_contract']} | {r['report_status']} | {r['changed_files']} |")


if __name__ == "__main__":
    main()
