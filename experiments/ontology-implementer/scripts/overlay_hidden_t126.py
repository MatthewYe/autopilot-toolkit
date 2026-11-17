#!/usr/bin/env python3
"""Overlay the merged T126 inline test modules onto a replay worktree.

The agent's implementation is kept; only the `#[cfg(test)] mod tests` block is
replaced with the merged version, so the agent cannot pass by rewriting tests.
"""
from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

REPO = Path(os.environ.get("AUTOPILOT_REPO", Path(__file__).resolve().parents[3]))
MERGE = "e3818a8"
FILES = [
    "crates/skill-index/src/lib.rs",
    "crates/validation-runner/src/lib.rs",
]


def merged(path: str) -> str:
    return subprocess.run(
        ["git", "-C", str(REPO), "show", f"{MERGE}:{path}"],
        text=True, capture_output=True, check=True,
    ).stdout


def split_tests(text: str):
    marker = "#[cfg(test)]\nmod tests {"
    idx = text.find(marker)
    if idx < 0:
        raise SystemExit("test module not found")
    return text[:idx], text[idx:]


def main():
    repo = Path(sys.argv[1]).resolve()
    for rel in FILES:
        target = repo / rel
        head, _ = split_tests(target.read_text())
        _, tests = split_tests(merged(rel))
        target.write_text(head + tests)
        print(f"overlaid tests: {rel}")


if __name__ == "__main__":
    main()
