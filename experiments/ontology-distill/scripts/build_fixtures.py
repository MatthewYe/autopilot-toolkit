#!/usr/bin/env python3
"""Build the gate fixtures from the real extracted corpus graph.

Every fixture is generated from a corpus artifact and then mutated in exactly
one way, so a failing fixture proves the shape that should have caught it.  The
expected outcome is written to ``fixtures/EXPECTED.json`` next to the graphs.

Run from the experiment root:  python3 scripts/build_fixtures.py
"""
from __future__ import annotations

import copy
import json
from pathlib import Path

from extract import (
    ROOT,
    extract_artifact,
    extract_issues_from_payloads,
    extract_prd_text,
    issues_ttl,
    load_manifest,
    prd_ttl,
)

FIXTURES = ROOT / "fixtures"


def _prd_base() -> dict:
    entry = next(e for e in load_manifest() if e["id"] == "prd-121")
    return extract_artifact(entry)


def _issues_base() -> dict:
    entry = next(e for e in load_manifest() if e["id"] == "issues-121")
    return extract_artifact(entry)


def _prd_source_text() -> str:
    return (ROOT / "corpus" / "specs" / "121-expected-set-resolves-skill-files.md").read_text()


def _empty_section(text: str, heading: str) -> str:
    """Keep the level-2 heading, drop the body up to the next heading."""
    out, skipping = [], False
    for line in text.splitlines():
        if line.startswith("## "):
            skipping = line.strip() == f"## {heading}"
            out.append(line)
        elif not skipping:
            out.append(line)
    return "\n".join(out) + "\n"


def _issue_payloads() -> list[dict]:
    return [
        json.loads((ROOT / "corpus" / "issues" / f"{n}.json").read_text())
        for n in (122, 123, 124, 125, 126)
    ]


def _drop_blocked_by(payload: dict) -> dict:
    out, skipping = [], False
    for line in (payload.get("body") or "").splitlines():
        if line.startswith("## "):
            skipping = line.strip() == "## Blocked by"
            if skipping:
                continue
        if not skipping:
            out.append(line)
    mutated = copy.deepcopy(payload)
    mutated["body"] = "\n".join(out)
    return mutated


def build() -> dict:
    FIXTURES.mkdir(exist_ok=True)
    expected: dict[str, dict] = {}

    def emit(name: str, ttl: str, tier: str, passes: bool, notes: str,
             advisory_count: int = 0, undetermined: bool = False) -> None:
        (FIXTURES / f"{name}.ttl").write_text(ttl)
        expected[name] = {
            "tier": tier,
            "pass": passes,
            "advisory_count": advisory_count,
            "undetermined": undetermined,
            "note": notes,
        }

    base = _prd_base()
    issues = _issues_base()

    emit("prd-valid", prd_ttl(base), "blocking", True,
         "Real #121 spec: all structural invariants hold.")
    emit("issues-valid", issues_ttl(issues), "blocking", True,
         "Real #122-#126 set: parents, acceptance criteria and DAG all hold.")

    # Markdown-level regressions: extract from text, not from a pre-built graph.
    for section in ("Problem Statement", "Solution"):
        feature = extract_prd_text(
            _empty_section(_prd_source_text(), section),
            {}, f"prd-empty-{section.split()[0].lower()}", "mutated #121 markdown")
        emit(f"prd-empty-{section.split()[0].lower()}", prd_ttl(feature),
             "blocking", False,
             f"#121 with the body of '{section}' emptied: P01/P02 must reject.")

    payloads = _issue_payloads()
    payloads[0] = _drop_blocked_by(payloads[0])
    undeclared = extract_issues_from_payloads(
        payloads, {"multiSlice": True}, "issues-no-blocked-by", "mutated #122 set")
    emit("issues-no-blocked-by", issues_ttl(undeclared), "blocking", False,
         "#122 loses its 'Blocked by' section: the DAG is unverifiable and T03 must reject.")

    # ── blocking mutations ────────────────────────────────────────────────
    cases = [
        ("prd-missing-problem", "sections",
         lambda f: f.__setitem__("sections", [s for s in f["sections"] if s != "Problem Statement"]),
         "P01"),
        ("prd-stories-unnumbered", "numberedStoryCount",
         lambda f: f.__setitem__("numberedStoryCount", 0), "P03"),
        ("prd-no-seam", "hasTestingSeam",
         lambda f: f.__setitem__("hasTestingSeam", False), "P05"),
        ("prd-no-out-of-scope", "outOfScopeItemCount",
         lambda f: f.__setitem__("outOfScopeItemCount", 0), "P06"),
    ]
    for name, _field, mutate, rule in cases:
        feature = copy.deepcopy(base)
        mutate(feature)
        emit(name, prd_ttl(feature), "blocking", False,
             f"Mutation of #121: {rule} must reject the draft.")

    issue_cases = [
        ("issues-missing-parent", lambda s: s["issues"][1].__setitem__("hasParent", False), "T04"),
        ("issues-no-ac", lambda s: s["issues"][2].__setitem__("acCount", 0), "T06"),
        ("issues-cycle", lambda s: s.__setitem__("hasCycle", True), "T03"),
        ("issues-dangling", lambda s: s.__setitem__("hasDanglingBlocker", True), "T03"),
    ]
    for name, mutate, rule in issue_cases:
        feature = copy.deepcopy(issues)
        mutate(feature)
        emit(name, issues_ttl(feature), "blocking", False,
             f"Mutation of #122-#126: {rule} must reject the set.")

    # ── advisory fixtures ─────────────────────────────────────────────────
    clean = copy.deepcopy(base)
    clean.update({
        "filePathCount": 0,
        "mentionsValidationCommand": True,
        "crossRuntime": False,
        "contractChange": False,
        "skillsChange": False,
        "ciChange": False,
    })
    emit("prd-advisory-clean", prd_ttl(clean), "advisory", True,
         "No advisory rule applies.", advisory_count=0)

    advisory_cases = [
        ("prd-advisory-files", "filePathCount", 8, "P04"),
        ("prd-advisory-runtime", "runtimePlan", False, "P10"),
        ("prd-advisory-migration", "mentionsMigration", False, "P11"),
        ("prd-advisory-validation", "mentionsValidationCommand", False, "P12"),
        ("prd-advisory-ci", "mentionsCiTrigger", False, "P13"),
        ("prd-advisory-stories", "wellFormedStoryCount", 10, "P03"),
    ]
    for name, field, value, rule in advisory_cases:
        feature = copy.deepcopy(clean)
        if field == "wellFormedStoryCount":
            feature["wellFormedStoryCount"] = value
        elif field == "filePathCount":
            feature["filePathCount"] = value
        elif field == "runtimePlan":
            feature["crossRuntime"] = True
            feature["runtimePlan"] = False
        elif field == "mentionsMigration":
            feature["contractChange"] = True
            feature["mentionsMigration"] = False
        elif field == "mentionsValidationCommand":
            feature["skillsChange"] = True
            feature["mentionsValidationCommand"] = False
        elif field == "mentionsCiTrigger":
            feature["ciChange"] = True
            feature["mentionsCiTrigger"] = False
        emit(name, prd_ttl(feature), "advisory", True,
             f"Advisory case: {rule} reports exactly one warning.",
             advisory_count=1)

    # ── fail-closed fixture ───────────────────────────────────────────────
    empty = (
        "@prefix d: <https://example.org/autopilot/distill#> .\n"
        "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n"
    )
    emit("graph-empty", empty, "blocking", False,
         "A graph with no focus node makes SHACL conformance undetermined; "
         "the gate must fail closed instead of treating it as a pass.",
         undetermined=True)

    (FIXTURES / "EXPECTED.json").write_text(
        json.dumps(expected, indent=2, sort_keys=True) + "\n"
    )
    return expected


if __name__ == "__main__":
    built = build()
    for name, want in sorted(built.items()):
        verdict = "pass" if want["pass"] else "reject"
        print(f"{name:32} {want['tier']:9} {verdict:7} {want['note']}")
    print(f"\n{len(built)} fixtures written to {FIXTURES}")
