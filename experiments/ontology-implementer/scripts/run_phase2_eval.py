#!/usr/bin/env python3
"""Evaluate finished replay runs without modifying the agent worktree.

The agent's diff (including untracked files) and report are snapshotted first.
Hidden tests are then overlaid on a temporary copy, so the original worktree
and the original diff stay intact.
"""
from __future__ import annotations

import argparse
import json
import os
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


def baseline_commit(repo: Path, run_dir: Path) -> str:
    """Return a commit object that exists in this sanitized repo.

    The prepared repo is history-free: its root commit has the base tree but
    not the original base object.  Prefer the real base when present, otherwise
    use the root commit.
    """
    roots = git(repo, "rev-list", "--max-parents=0", "HEAD").split()
    if not roots:
        raise SystemExit(f"cannot determine baseline for {repo}")
    meta = run_dir / ".replay-base"
    if meta.exists():
        candidate = json.loads(meta.read_text())["commit"]
        if subprocess.run(
            ["git", "cat-file", "-e", f"{candidate}^{{commit}}"],
            cwd=repo, text=True, capture_output=True,
        ).returncode == 0:
            return candidate
        print(f"note: base {candidate[:8]} not in sanitized repo; using root {roots[-1][:8]}")
        return roots[-1]
    return roots[-1]


def snapshot_agent(repo: Path, base: str) -> tuple[str, list[str]]:
    """Return (binary diff from base, changed paths) without touching the index.

    The agent may have committed its work.  Diffing against HEAD would then
    return an empty patch.  We use a throwaway index seeded from the prepared
    base commit, so committed and uncommitted changes are both captured and the
    real index is never reset.
    """
    fd, index_path = tempfile.mkstemp(prefix="replay-index-")
    os.close(fd)
    os.unlink(index_path)
    env = os.environ.copy()
    env["GIT_INDEX_FILE"] = index_path
    try:
        subprocess.run(["git", "read-tree", base], cwd=repo, env=env, check=True)
        subprocess.run(["git", "add", "-A", "--", ".", ":(exclude)corpus"], cwd=repo, env=env, check=True)
        diff = subprocess.run(
            ["git", "diff", "--cached", "--binary", base],
            cwd=repo, env=env, text=True, capture_output=True, check=True,
        ).stdout
        names = subprocess.run(
            ["git", "diff", "--cached", "--name-only", base],
            cwd=repo, env=env, text=True, capture_output=True, check=True,
        ).stdout
    finally:
        try:
            os.unlink(index_path)
        except FileNotFoundError:
            pass
    paths = [p for p in names.splitlines() if p and p != "IMPLEMENTER_REPORT.md" and not p.startswith("corpus/")]
    return diff, paths


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
        base = baseline_commit(repo, repo.parent)
        diff, agent_changed = snapshot_agent(repo, base)
        (out_dir / f"{run}-agent.diff").write_text(diff)
        print(f"== {run}: snapshotted agent diff from {base[:8]} ({len(diff)} bytes)")

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
            "agent_base": base,
            "hidden_tests_commit": HIDDEN_TESTS_COMMIT,
        })
        result_path.write_text(json.dumps(result, indent=2))
        print(f"{run}: report={report['pass']} structural={structural['pass']}")


if __name__ == "__main__":
    main()
