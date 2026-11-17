#!/usr/bin/env python3
"""Evaluate one Phase 2 replay worktree.

Runs the repository checks that the merged baseline was verified with and
records a machine-readable result. It never edits the worktree.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import time
from pathlib import Path

CHECKS = [
    ("workspace_tests", ["cargo", "test", "--workspace", "--no-fail-fast"], 2400),
    ("toolkit_setup_tests", ["rust-script", "--test", "tests/test_toolkit_setup.rs"], 900),
    ("skill_check", ["rust-script", "scripts/check.rs"], 900),
    ("validation", ["rust-script", "validation/run.rs"], 900),
    ("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"], 1800),
]


def run(cmd, cwd, timeout):
    started = time.time()
    try:
        p = subprocess.run(cmd, cwd=cwd, text=True, capture_output=True, timeout=timeout)
        return {
            "cmd": " ".join(cmd),
            "exit": p.returncode,
            "seconds": round(time.time() - started, 1),
            "stdout_tail": p.stdout[-4000:],
            "stderr_tail": p.stderr[-4000:],
        }
    except subprocess.TimeoutExpired as exc:
        return {
            "cmd": " ".join(cmd),
            "exit": 124,
            "seconds": round(time.time() - started, 1),
            "stdout_tail": (exc.stdout or "")[-2000:] if isinstance(exc.stdout, str) else "",
            "stderr_tail": f"timeout after {timeout}s",
        }


def git(repo, *args):
    p = subprocess.run(["git", *args], cwd=repo, text=True, capture_output=True)
    return p.stdout.strip() if p.returncode == 0 else ""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--skip", default="", help="comma-separated check names to skip")
    args = ap.parse_args()
    repo = Path(args.repo).resolve()
    skip = {s for s in args.skip.split(",") if s}

    result = {
        "repo": str(repo),
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "head": git(repo, "rev-parse", "HEAD"),
        "status": git(repo, "status", "--short"),
        "diff_stat": git(repo, "diff", "--stat", "HEAD"),
        "changed_files": [x for x in git(repo, "diff", "--name-only", "HEAD").splitlines() if x],
        "report_present": (repo / "IMPLEMENTER_REPORT.md").exists(),
        "checks": {},
    }
    for name, cmd, timeout in CHECKS:
        if name in skip:
            continue
        print(f"== {name}: {' '.join(cmd)}", flush=True)
        r = run(cmd, repo, timeout)
        result["checks"][name] = r
        print(f"   exit={r['exit']} seconds={r['seconds']}", flush=True)
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, indent=2))
    print(f"wrote {out}")
    print(json.dumps({k: v["exit"] for k, v in result["checks"].items()}, indent=2))


if __name__ == "__main__":
    main()
