#!/usr/bin/env python3
"""Evaluate finished replay runs: snapshot diff, overlay hidden tests, run checks."""
from __future__ import annotations

import argparse
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "results" / "phase2"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", default="", help="comma-separated run dirs, default all non-gold")
    ap.add_argument("--skip", default="")
    args = ap.parse_args()
    runs = [r for r in args.runs.split(",") if r] or sorted(
        p.name for p in (ROOT / "replay" / "T126").iterdir()
        if p.is_dir() and p.name != "gold"
    )
    OUT.mkdir(parents=True, exist_ok=True)
    for run in runs:
        repo = ROOT / "replay" / "T126" / run / "repo"
        if not repo.exists():
            print(f"skip missing {run}")
            continue
        diff = subprocess.run(["git", "diff", "--binary", "HEAD"], cwd=repo,
                              text=True, capture_output=True)
        (OUT / f"{run}-agent.diff").write_text(diff.stdout)
        print(f"== {run}: saved agent diff ({len(diff.stdout)} bytes)")
        subprocess.run(["python3", str(ROOT / "scripts/overlay_hidden_t126.py"), str(repo)], check=True)
        subprocess.run([
            "python3", str(ROOT / "scripts/evaluate_replay.py"),
            "--repo", str(repo), "--out", str(OUT / f"{run}.json"),
            "--skip", args.skip,
        ], check=True)


if __name__ == "__main__":
    main()
