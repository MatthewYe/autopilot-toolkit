#!/usr/bin/env python3
"""Write the injection-cost comparison from selection results."""
from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ARMS = ["none", "generic", "keyword", "ontology"]
MODES = ["full", "ids", "compact", "hybrid"]


def mean(values):
    values = list(values)
    return round(sum(values) / len(values), 4) if values else 0.0


def main():
    rows = json.loads((ROOT / "results" / "selection.json").read_text())
    arms = {}
    for arm in ARMS:
        arm_rows = [r for r in rows if r["arm"] == arm]
        arms[arm] = {
            mode: mean(r[f"chars_{mode}"] for r in arm_rows)
            for mode in MODES
        }
    payload = {
        "unit": "mean characters per task across T122-T126",
        "arms": arms,
        "note": "hybrid = full clause text + context path/line pointer, no context body; "
                "compact = clause ids + context titles; ids = ids only.",
    }
    (ROOT / "results" / "injection.json").write_text(json.dumps(payload, indent=2) + "\n")
    print(json.dumps(payload, indent=2))


if __name__ == "__main__":
    main()
