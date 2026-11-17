#!/usr/bin/env python3
"""Render the deterministic implementer plan for one or all tasks.

This is the actual injection payload shape: clause text (normative) plus context
pointers (path + line range + short excerpt), not the full context bodies.
"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib import Store, load_inventory, select

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "results"


def render(task: dict, clauses, contexts) -> str:
    lines = [
        f"# Implementer plan: {task['id']}",
        "",
        "## Task facts",
        "",
        f"- source: `{task['source']}`",
        f"- runtime: `{task['runtime']}`",
        f"- round: `{task['round']}`",
        f"- specLinked: `{str(task['specLinked']).lower()}`",
        f"- multiModule: `{str(task['multiModule']).lower()}`",
        f"- changeKinds: {', '.join(task['changeKinds'])}",
        f"- risks: {', '.join(task['risks'])}",
        "",
        f"## Clauses ({len(clauses)})",
        "",
    ]
    for clause in clauses:
        lines += [
            f"### {clause['id']} [{clause['axis']}]",
            "",
            clause["text"],
            "",
            f"Report check: `{clause['check']}`  ",
            f"Source: `{clause['anchor']}`",
            "",
        ]
    lines += [f"## Context ({len(contexts)})", ""]
    for ctx in contexts:
        lines.append(f"- `{ctx['path']}:{ctx['start']}-{ctx['end']}` — {ctx['title']}")
        excerpt = " ".join(ctx["excerpt"].split())
        if excerpt:
            lines.append(f"  > {excerpt[:180]}")
    lines += [
        "",
        "## Report contract",
        "",
        "Start with `IMPLEMENTER_REPORT:` and include `ROUND`, `STATUS`, "
        "`SUGGESTION_RESOLUTIONS`, `SELF_REVIEW`, `CHANGED_FILES`, `SUMMARY`.",
        "Every clause above needs a check in the report; a production change needs "
        "TDD evidence; DONE requires executed passing evidence for every AC.",
        "",
    ]
    return "\n".join(lines)


def main(argv):
    store = Store([])
    clauses, tasks, contexts = load_inventory(store)
    store.close()
    ids = argv or sorted(tasks)
    OUT.mkdir(exist_ok=True)
    for tid in ids:
        if tid not in tasks:
            raise SystemExit(f"unknown task: {tid}")
        task = tasks[tid]
        selected_clauses, selected_contexts = select(clauses, contexts, task)
        text = render(task, selected_clauses, selected_contexts)
        path = OUT / f"plan-{tid}.md"
        path.write_text(text)
        print(f"wrote {path} ({len(text)} chars, {len(selected_clauses)} clauses, "
              f"{len(selected_contexts)} contexts)")


if __name__ == "__main__":
    main(sys.argv[1:])
