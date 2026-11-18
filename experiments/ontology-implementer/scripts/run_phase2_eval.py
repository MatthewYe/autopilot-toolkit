#!/usr/bin/env python3
"""Evaluate finished replay runs without modifying the agent worktree.

The agent's diff (including untracked files) and report are snapshotted first.
Hidden tests are then overlaid on a temporary copy, so the original worktree
and the original diff stay intact.
"""
from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "results" / "phase2"
HIDDEN_TESTS_COMMIT = "e3818a8"

sys.path.insert(0, str(ROOT / "scripts"))
from report_contract import parse_report  # noqa: E402


def git(repo: Path, *args: str) -> str:
    p = subprocess.run(["git", *args], cwd=repo, text=True, capture_output=True)
    return p.stdout if p.returncode == 0 else ""


def snapshot_agent_diff(repo: Path) -> str:
    # -N makes untracked files visible to git diff; reset clears only the
    # index entry, not the working tree.
    subprocess.run(["git", "add", "-N", "."], cwd=repo, check=True)
    try:
        return git(repo, "diff", "--binary", "HEAD")
    finally:
        subprocess.run(["git", "reset", "-q"], cwd=repo, check=True)


def changed_paths(repo: Path) -> list[str]:
    out = []
    for line in git(repo, "status", "--porcelain").splitlines():
        path = line[3:]
        if " -> " in path:
            path = path.split(" -> ", 1)[1]
        if path and path != "IMPLEMENTER_REPORT.md":
            out.append(path)
    return out


def copy_without_git(repo: Path, dest: Path) -> None:
    shutil.copytree(repo, dest, ignore=shutil.ignore_patterns(".git"))


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", default="", help="comma-separated run dirs, default all non-gold")
    ap.add_argument("--root", default=str(ROOT / "replay"))
    ap.add_argument("--out-dir", default=str(OUT))
    ap.add_argument("--skip", default="")
    args = ap.parse_args()
    runs = [r for r in args.runs.split(",") if r] or sorted(
        p.name for p in (Path(args.root) / "T126").iterdir()
        if p.is_dir() and p.name != "gold"
    )
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    for run in runs:
        repo = Path(args.root) / "T126" / run / "repo"
        if not repo.exists():
            print(f"skip missing {run}")
            continue
        diff = snapshot_agent_diff(repo)
        (out_dir / f"{run}-agent.diff").write_text(diff)
        print(f"== {run}: snapshotted agent diff ({len(diff)} bytes)")

        agent_changed = changed_paths(repo)
        report_text = (repo / "IMPLEMENTER_REPORT.md").read_text() if (repo / "IMPLEMENTER_REPORT.md").exists() else ""
        report = parse_report(report_text)

        structural_run = subprocess.run(
            ["python3", str(ROOT / "scripts/check_t126_contract.py"), str(repo)],
            text=True, capture_output=True,
        )
        structural = {
            "exit": structural_run.returncode,
            "pass": structural_run.returncode == 0,
            "stdout": structural_run.stdout,
            "stderr": structural_run.stderr,
        }

        with tempfile.TemporaryDirectory(prefix=f"t126-{run}-") as td:
            overlay_repo = Path(td) / "repo"
            copy_without_git(repo, overlay_repo)
            # The overlay copy has no history; some workspace tests call
            # `git rev-parse`. Initialize a throwaway repo so those tests see a
            # real commit without copying the agent's .git directory.
            subprocess.run(["git", "init", "-q"], cwd=overlay_repo, check=True)
            subprocess.run(["git", "add", "-A"], cwd=overlay_repo, check=True)
            subprocess.run([
                "git", "-c", "user.name=replay", "-c", "user.email=replay@example.invalid",
                "commit", "-qm", "overlay",
            ], cwd=overlay_repo, check=True)
            subprocess.run(
                ["python3", str(ROOT / "scripts/overlay_hidden_t126.py"), str(overlay_repo)],
                check=True,
            )
            result_path = out_dir / f"{run}.json"
            subprocess.run([
                "python3", str(ROOT / "scripts/evaluate_replay.py"),
                "--repo", str(overlay_repo), "--out", str(result_path),
                "--run", run, "--skip", args.skip,
                "--hidden-tests-commit", HIDDEN_TESTS_COMMIT,
            ], check=True)
            result = json.loads(result_path.read_text())

        result.update({
            "run": run,
            "agent_diff": f"{run}-agent.diff",
            "report": report,
            "report_text": report_text,
            "structural": structural,
            "changed_files": agent_changed,
            "hidden_tests_commit": HIDDEN_TESTS_COMMIT,
        })
        result_path.write_text(json.dumps(result, indent=2))
        print(f"{run}: report={report['pass']} structural={structural['pass']}")


if __name__ == "__main__":
    main()
