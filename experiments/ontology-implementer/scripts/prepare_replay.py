#!/usr/bin/env python3
"""Create isolated, history-free replay worktrees for one ticket.

The export is prepared in a temporary directory and moved into place only
after the source commits have been verified and the archive succeeded.  An
existing run is preserved unless --force is passed.
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPO = Path(os.environ.get("AUTOPILOT_REPO", Path(__file__).resolve().parents[3]))


def git(*args: str, cwd: Path | None = None) -> subprocess.CompletedProcess:
    return subprocess.run(["git", *args], cwd=cwd, text=True, capture_output=True)


def toplevel(repo: Path) -> Path:
    result = git("-C", str(repo), "rev-parse", "--show-toplevel")
    if result.returncode != 0:
        raise SystemExit(f"not a git worktree: {repo}\n{result.stderr.strip()}")
    return Path(result.stdout.strip())


def ensure_commit(repo: Path, commit: str, role: str) -> str:
    result = git("-C", str(repo), "rev-parse", "--verify", f"{commit}^{{commit}}")
    if result.returncode == 0:
        return result.stdout.strip()
    print(f"{role} {commit} missing locally; fetching PR refs", file=sys.stderr)
    git("-C", str(repo), "fetch", "origin", "refs/pull/127/head:refs/remotes/origin/pr-127")
    result = git("-C", str(repo), "rev-parse", "--verify", f"{commit}^{{commit}}")
    if result.returncode != 0:
        raise SystemExit(
            f"{role} commit {commit} is not available in {repo}; "
            "fetch the source branch first (no run directory was touched)"
        )
    return result.stdout.strip()


def export(repo: Path, commit: str, dest: Path) -> None:
    dest.mkdir(parents=True)
    archive = subprocess.Popen(
        ["git", "-C", str(repo), "archive", commit],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    extract = subprocess.Popen(
        ["tar", "-x", "-C", str(dest)],
        stdin=archive.stdout, stderr=subprocess.PIPE,
    )
    assert archive.stdout is not None
    archive.stdout.close()
    _, extract_err = extract.communicate()
    _, archive_err = archive.communicate()
    if archive.returncode != 0 or extract.returncode != 0:
        raise SystemExit(
            f"git archive {commit} failed: "
            f"{archive_err.decode(errors='replace').strip()} "
            f"{extract_err.decode(errors='replace').strip()}"
        )


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--task", default="126")
    ap.add_argument("--base", default="98270ef")
    ap.add_argument("--gold", default="e3818a8")
    ap.add_argument("--reps", type=int, default=2)
    ap.add_argument("--arms", default="prose,ontology")
    ap.add_argument("--root", default=str(ROOT / "replay"))
    ap.add_argument("--force", action="store_true", help="replace an existing run")
    args = ap.parse_args()

    repo = toplevel(REPO)
    base = ensure_commit(repo, args.base, "base")
    gold = ensure_commit(repo, args.gold, "hidden-test")

    issue = json.loads((ROOT / "corpus" / f"issue-{args.task}.json").read_text())
    task_text = f"# {issue['title']}\n\n{issue['body']}\n"
    plan_path = ROOT / "results" / f"plan-T{args.task}.md"
    plan_text = plan_path.read_text() if plan_path.exists() else ""
    if "ontology" in args.arms.split(",") and not plan_text:
        raise SystemExit(f"missing ontology plan: {plan_path}")

    for arm in args.arms.split(","):
        for rep in range(1, args.reps + 1):
            dest = Path(args.root) / f"T{args.task}" / f"{arm}-{rep}"
            if dest.exists() and not args.force:
                print(f"skip existing {dest} (use --force to replace)")
                continue
            tmp = dest.parent / f".{dest.name}.prepare-{os.getpid()}"
            if tmp.exists():
                shutil.rmtree(tmp)
            try:
                repo_dir = tmp / "repo"
                export(repo, base, repo_dir)
                subprocess.run(["git", "init", "-q"], cwd=repo_dir, check=True)
                subprocess.run(["git", "config", "user.email", "replay@example.invalid"], cwd=repo_dir, check=True)
                subprocess.run(["git", "config", "user.name", "Replay"], cwd=repo_dir, check=True)
                subprocess.run(["git", "add", "."], cwd=repo_dir, check=True)
                subprocess.run(["git", "commit", "-q", "-m", f"base {base}"], cwd=repo_dir, check=True)
                (tmp / "TASK.md").write_text(task_text)
                if arm == "ontology":
                    (tmp / "ONTOLOGY_PLAN.md").write_text(plan_text)
                if dest.exists():
                    backup = dest.parent / f".{dest.name}.backup-{os.getpid()}"
                    if backup.exists():
                        shutil.rmtree(backup)
                    dest.rename(backup)
                    tmp.rename(dest)
                    shutil.rmtree(backup)
                else:
                    tmp.rename(dest)
                print(f"{dest} (base {base[:8]}, hidden {gold[:8]})")
            finally:
                if tmp.exists():
                    shutil.rmtree(tmp)


if __name__ == "__main__":
    main()
