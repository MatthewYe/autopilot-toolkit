#!/usr/bin/env python3
"""Aggregate selection and injection results for the implementer experiment."""
from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ARMS = ["none", "generic", "keyword", "ontology"]


def mean(values):
    values = list(values)
    return round(sum(values) / len(values), 4) if values else 0.0


def main():
    rows = json.loads((ROOT / "results" / "selection.json").read_text())
    aggregate = []
    for arm in ARMS:
        arm_rows = [r for r in rows if r["arm"] == arm]
        aggregate.append({
            "arm": arm,
            "runs": len(arm_rows),
            "clause_recall": mean(r["clause_recall"] for r in arm_rows),
            "clause_precision": mean(r["clause_precision"] for r in arm_rows),
            "context_recall": mean(r["context_recall"] for r in arm_rows),
            "context_precision": mean(r["context_precision"] for r in arm_rows),
            "exact_runs": sum(1 for r in arm_rows if r["exact"]),
            "clause_selected": sum(r["clause_selected"] for r in arm_rows),
            "clause_gold": sum(r["clause_gold"] for r in arm_rows),
            "context_selected": sum(r["context_selected"] for r in arm_rows),
            "context_gold": sum(r["context_gold"] for r in arm_rows),
            "chars_full": mean(r["chars_full"] for r in arm_rows),
            "chars_ids": mean(r["chars_ids"] for r in arm_rows),
            "chars_compact": mean(r["chars_compact"] for r in arm_rows),
            "chars_hybrid": mean(r["chars_hybrid"] for r in arm_rows),
            "chars_rendered": mean(r.get("chars_rendered", 0) for r in arm_rows),
        })
    (ROOT / "results" / "aggregate.json").write_text(json.dumps(aggregate, indent=2) + "\n")
    print(json.dumps(aggregate, indent=2))


if __name__ == "__main__":
    main()
