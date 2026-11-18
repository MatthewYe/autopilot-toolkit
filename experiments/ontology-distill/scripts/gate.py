"""Run one extracted Draft graph against the Distill gate shapes."""
from __future__ import annotations

import tempfile
from pathlib import Path

from lib import ROOT, Store

SHAPES = {
    "blocking": ROOT / "shapes" / "gate-shapes.ttl",
    "advisory": ROOT / "shapes" / "advisory-shapes.ttl",
}


def evaluate(ttl: str, tier: str, name: str = "draft") -> dict:
    """Return {pass, undetermined, violations, exit_code} for one graph."""
    if tier not in SHAPES:
        raise ValueError(f"unknown tier {tier!r}")
    with tempfile.TemporaryDirectory(prefix="distill-fixture-") as tmp:
        path = Path(tmp) / f"{name}.ttl"
        path.write_text(ttl)
        store = Store([path])
        try:
            result = store.shacl(SHAPES[tier])
        finally:
            store.close()

    violations = [
        {
            "message": item.get("message", ""),
            "focus": item.get("focus_node", ""),
            "severity": item.get("severity", "Violation"),
        }
        for item in result["violations"]
    ]
    undetermined = result["exit_code"] == 2 or result["conforms"] is None
    if undetermined:
        passed = False
        violations = violations or [
            {
                "message": "SHACL conformance is undetermined: no focus node matched a shape.",
                "focus": "",
                "severity": "Undetermined",
            }
        ]
    else:
        passed = result["exit_code"] == 0
    return {
        "pass": passed,
        "undetermined": undetermined,
        "violations": violations,
        "exit_code": result["exit_code"],
    }
