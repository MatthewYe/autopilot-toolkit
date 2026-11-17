#!/usr/bin/env python3
"""Run the two rust-script integration suites on already-overlaid replay worktrees."""
from __future__ import annotations

import argparse
import json
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "results" / "phase2"
CHECKS = [
    ("test_build", ["rust-script", "--test", "tests/test_build.rs"], 1800),
    ("test_install", ["rust-script", "--test", "tests/test_install.rs"], 2400),
]


def run(cmd, cwd, timeout):
    t = time.time()
    try:
        p = subprocess.run(cmd, cwd=cwd, text=True, capture_output=True, timeout=timeout)
        return {"cmd": " ".join(cmd), "exit": p.returncode,
                "seconds": round(time.time() - t, 1),
                "stdout_tail": p.stdout[-3000:], "stderr_tail": p.stderr[-3000:]}
    except subprocess.TimeoutExpired as exc:
        return {"cmd": " ".join(cmd), "exit": 124,
                "seconds": round(time.time() - t, 1),
                "stdout_tail": "", "stderr_tail": f"timeout after {timeout}s"}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", default="prose-1,prose-2,ontology-1,ontology-2")
    args = ap.parse_args()
    for run_name in args.runs.split(","):
        repo = ROOT / "replay" / "T126" / run_name / "repo"
        out = OUT / f"{run_name}-integration.json"
        data = {"run": run_name, "checks": {}}
        for name, cmd, timeout in CHECKS:
            print(f"== {run_name} {name}", flush=True)
            r = run(cmd, repo, timeout)
            print(f"   exit={r['exit']} seconds={r['seconds']}", flush=True)
            data["checks"][name] = r
        out.write_text(json.dumps(data, indent=2))
        print(f"wrote {out}", flush=True)


if __name__ == "__main__":
    main()
