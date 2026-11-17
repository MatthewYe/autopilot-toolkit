#!/usr/bin/env python3
"""Create isolated, history-free replay worktrees for one ticket."""
from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPO = Path(os.environ.get("AUTOPILOT_REPO", Path(__file__).resolve().parents[3]))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--task", default="126")
    ap.add_argument("--base", default="98270ef")
    ap.add_argument("--reps", type=int, default=2)
    ap.add_argument("--arms", default="prose,ontology")
    ap.add_argument("--root", default=str(ROOT / "replay"))
    args = ap.parse_args()

    issue = json.loads((ROOT / "corpus" / f"issue-{args.task}.json").read_text())
    task_text = f"# {issue['title']}\n\n{issue['body']}\n"
    plan_path = ROOT / "results" / f"plan-T{args.task}.md"
    plan_text = plan_path.read_text() if plan_path.exists() else ""

    for arm in args.arms.split(","):
        for rep in range(1, args.reps + 1):
            dest = Path(args.root) / f"T{args.task}" / f"{arm}-{rep}"
            repo = dest / "repo"
            if dest.exists():
                shutil.rmtree(dest)
            repo.mkdir(parents=True)
            subprocess.run(
                f"git -C {REPO} archive {args.base} | tar -x -C {repo}",
                shell=True, check=True,
            )
            subprocess.run(["git", "init", "-q"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.email", "replay@example.invalid"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.name", "Replay"], cwd=repo, check=True)
            subprocess.run(["git", "add", "."], cwd=repo, check=True)
            subprocess.run(["git", "commit", "-q", "-m", f"base {args.base}"], cwd=repo, check=True)
            (dest / "TASK.md").write_text(task_text)
            if arm == "ontology":
                (dest / "ONTOLOGY_PLAN.md").write_text(plan_text)
            print(dest)


if __name__ == "__main__":
    main()
