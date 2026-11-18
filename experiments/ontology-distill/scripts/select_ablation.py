#!/usr/bin/env python3
"""Deterministic selection ablation: none / generic / keyword / ontology.

This measures what each injection strategy would put in front of the executor,
against a hand-labelled gold pack.  It does not call a model.
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib import (Store, chars, estimate_tokens, load_inventory, metrics, select)

ROOT = Path(__file__).resolve().parents[1]
RESULTS = ROOT / "results"
STAGES = ["Clarification", "Prd", "Issues"]
STOPWORDS = {
    "about", "after", "again", "against", "agent", "allow", "already", "also",
    "another", "because", "before", "being", "between", "both", "change",
    "changes", "could", "current", "does", "each", "every", "from", "have",
    "into", "itself", "keep", "keeping", "make", "makes", "must", "never",
    "only", "other", "over", "same", "should", "skill", "skills", "some",
    "such", "that", "their", "them", "then", "there", "these", "they", "this",
    "those", "through", "under", "until", "using", "want", "when", "where",
    "which", "while", "with", "without", "would", "your",
}


def tokens(text: str) -> set[str]:
    return {
        word
        for word in re.findall(r"[a-z0-9]+", text.lower())
        if len(word) > 4 and word not in STOPWORDS
    }


def arm_none(clauses, contexts, task, stage):
    return [], []


def arm_generic(clauses, contexts, task, stage):
    cl = [c for c in clauses.values() if c["enforced"] is None]
    return cl, list(contexts.values())


def arm_stage(clauses, contexts, task, stage):
    """Stage packs only: every applicable clause and card, no task conditions."""
    cl = [c for c in clauses.values() if c["enforced"] is None and c["stage"] == stage]
    ctx = [x for x in contexts.values()
           if stage in x.get("stages", []) or "Cross" in x.get("stages", [])]
    return cl, ctx


def arm_keyword(clauses, contexts, task, stage):
    """Lexical retrieval capped at a typical pack size (12 clauses, 6 contexts)."""
    bag = tokens(task["title"] + " " + task["requirement"])

    def ranked(pool, text_of, limit):
        scored = []
        for item in pool:
            score = len(tokens(text_of(item)) & bag)
            if score:
                scored.append((score, item))
        scored.sort(key=lambda pair: (-pair[0], pair[1].get("id", "")))
        return [item for _, item in scored[:limit]]

    cl = ranked(
        [c for c in clauses.values() if c["enforced"] is None and c["stage"] == stage],
        lambda c: c["text"] + " " + c["keyword"], 12)
    ctx = ranked(
        [x for x in contexts.values()
         if stage in x.get("stages", []) or "Cross" in x.get("stages", [])],
        lambda x: x["title"] + " " + x["path"] + " " + x["excerpt"], 6)
    return cl, ctx


def arm_ontology(clauses, contexts, task, stage):
    return select(clauses, contexts, task, stage)


ARMS = {
    "none": arm_none,
    "generic": arm_generic,
    "stage": arm_stage,
    "keyword": arm_keyword,
    "ontology": arm_ontology,
}


def render_payload(task: dict, stage: str, clause_list, context_list) -> str:
    lines = [
        f"# Distill context pack: {task['id']} / {stage}",
        "",
        "## Task facts",
        "",
        f"- requirement: {task['requirement']}",
        f"- change kinds: {', '.join(task.get('changeKinds', []))}",
        f"- surfaces: {', '.join(task.get('surfaces', []))}",
        f"- runtimes: {', '.join(task.get('runtimes', [])) or '—'}",
        f"- multi-module: {str(task.get('multiModule')).lower()}; "
        f"multi-slice: {str(task.get('multiSlice')).lower()}; "
        f"cross-runtime: {str(task.get('crossRuntime')).lower()}",
        "",
        f"## Clauses ({len(clause_list)})",
        "",
    ]
    for clause in clause_list:
        lines += [f"### {clause['id']} [{clause['axis']}]", "", clause["text"], ""]
    lines += [f"## Context pointers ({len(context_list)})", ""]
    for context in context_list:
        lines.append(
            f"- `{context['path']}:{context['start']}-{context['end']}` — {context['title']}"
        )
        lines.append(f"  > {' '.join(context['excerpt'].split())[:220]}")
    return "\n".join(lines) + "\n"


def main() -> int:
    store = Store([])
    clauses, tasks_raw, contexts = load_inventory(store)
    store.close()
    tasks = {task["id"]: task for task in tasks_raw.values()}
    gold = json.loads((ROOT / "data" / "gold.json").read_text())

    rows = []
    for task_id in sorted(tasks):
        task = tasks[task_id]
        for stage in STAGES:
            expected = gold["tasks"][task_id][stage]
            for arm, fn in ARMS.items():
                clause_list, context_list = fn(clauses, contexts, task, stage)
                clause_ids = sorted(c["id"] for c in clause_list)
                context_ids = sorted(c["id"] for c in context_list)
                c_recall, c_precision, c_hit, c_sel, c_gold = metrics(
                    clause_ids, expected["clauses"])
                x_recall, x_precision, x_hit, x_sel, x_gold = metrics(
                    context_ids, expected["contexts"])
                rows.append({
                    "task": task_id,
                    "stage": stage,
                    "arm": arm,
                    "clause_ids": clause_ids,
                    "context_ids": context_ids,
                    "clause_recall": round(c_recall, 4),
                    "clause_precision": round(c_precision, 4),
                    "context_recall": round(x_recall, 4),
                    "context_precision": round(x_precision, 4),
                    "combined_recall": round((c_hit + x_hit) / (c_gold + x_gold), 4),
                    "combined_precision": round((c_hit + x_hit) / (c_sel + x_sel), 4)
                    if (c_sel + x_sel) else 0.0,
                    "exact": clause_ids == sorted(expected["clauses"])
                             and context_ids == sorted(expected["contexts"]),
                    "chars_full": chars(clause_list, context_list, "full"),
                    "chars_hybrid": chars(clause_list, context_list, "hybrid"),
                    "clause_chars": sum(len(c["text"]) for c in clause_list),
                    "context_chars": sum(len(x["excerpt"]) for x in context_list),
                })

    aggregates = {}
    for arm in ARMS:
        subset = [r for r in rows if r["arm"] == arm]
        aggregates[arm] = {
            "runs": len(subset),
            "clause_recall": round(sum(r["clause_recall"] for r in subset) / len(subset), 4),
            "clause_precision": round(sum(r["clause_precision"] for r in subset) / len(subset), 4),
            "context_recall": round(sum(r["context_recall"] for r in subset) / len(subset), 4),
            "context_precision": round(sum(r["context_precision"] for r in subset) / len(subset), 4),
            "combined_recall": round(sum(r["combined_recall"] for r in subset) / len(subset), 4),
            "combined_precision": round(sum(r["combined_precision"] for r in subset) / len(subset), 4),
            "exact": round(sum(r["exact"] for r in subset) / len(subset), 4),
            "mean_chars_full": round(sum(r["chars_full"] for r in subset) / len(subset)),
            "mean_chars_hybrid": round(sum(r["chars_hybrid"] for r in subset) / len(subset)),
            "mean_clause_chars": round(sum(r["clause_chars"] for r in subset) / len(subset)),
            "mean_context_chars": round(sum(r["context_chars"] for r in subset) / len(subset)),
        }
    per_stage = {}
    for stage in STAGES:
        per_stage[stage] = {}
        for arm in ARMS:
            subset = [r for r in rows if r["arm"] == arm and r["stage"] == stage]
            per_stage[stage][arm] = {
                "clause_recall": round(sum(r["clause_recall"] for r in subset) / len(subset), 4),
                "clause_precision": round(sum(r["clause_precision"] for r in subset) / len(subset), 4),
                "mean_chars_full": round(sum(r["chars_full"] for r in subset) / len(subset)),
            }

    payload_dir = RESULTS / "payloads"
    payload_dir.mkdir(exist_ok=True)
    for task_id in sorted(tasks):
        for stage in STAGES:
            clause_list, context_list = arm_ontology(clauses, contexts, tasks[task_id], stage)
            (payload_dir / f"{task_id}-{stage}.md").write_text(
                render_payload(tasks[task_id], stage, clause_list, context_list),
                encoding="utf-8")

    payload = {"aggregates": aggregates, "per_stage": per_stage, "rows": rows}
    (RESULTS / "selection.json").write_text(json.dumps(payload, indent=2) + "\n")

    lines = ["Distill selection ablation", ""]
    header = (f"{'arm':10s} {'recall':>7s} {'prec':>7s} {'ctxR':>7s} {'ctxP':>7s} "
              f"{'exact':>6s} {'chars':>7s} {'hybrid':>7s}")
    lines.append(header)
    lines.append("-" * len(header))
    for arm, agg in aggregates.items():
        lines.append(
            f"{arm:10s} {agg['clause_recall']:7.3f} {agg['clause_precision']:7.3f} "
            f"{agg['context_recall']:7.3f} {agg['context_precision']:7.3f} "
            f"{agg['exact']:6.3f} {agg['mean_chars_full']:7d} {agg['mean_chars_hybrid']:7d}")
    lines += ["", f"{'stage':14s} {'arm':10s} {'recall':>7s} {'prec':>7s} {'chars':>7s}"]
    for stage in STAGES:
        for arm in ARMS:
            agg = per_stage[stage][arm]
            lines.append(f"{stage:14s} {arm:10s} {agg['clause_recall']:7.3f} "
                         f"{agg['clause_precision']:7.3f} {agg['mean_chars_full']:7d}")
    text = "\n".join(lines)
    (RESULTS / "selection.txt").write_text(text + "\n")

    md = ["| Arm | Clause recall | Clause precision | Context recall | Context precision | Exact set | Mean full chars | Mean hybrid chars |",
          "|---|---|---|---|---|---|---|---|"]
    for arm, agg in aggregates.items():
        md.append(f"| {arm} | {agg['clause_recall']:.3f} | {agg['clause_precision']:.3f} "
                  f"| {agg['context_recall']:.3f} | {agg['context_precision']:.3f} "
                  f"| {agg['exact']:.3f} | {agg['mean_chars_full']} | {agg['mean_chars_hybrid']} |")
    (RESULTS / "selection.md").write_text("\n".join(md) + "\n")

    print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
