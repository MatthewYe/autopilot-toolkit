#!/usr/bin/env python3
"""Run the gate against its fixtures and replay the real corpus.

The fixture table is a unit test: every mutation must be rejected by the rule
named in its note.  The replay table is the calibration evidence: every
artifact a human already approved must pass the blocking tier, otherwise the
gate is stricter than practice and would block work that shipped.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

from extract import ROOT, extract_artifact, load_manifest, ttl_for
from gate import evaluate

RESULTS = ROOT / "results"
FIXTURES = ROOT / "fixtures"


def run_fixtures() -> tuple[list[dict], bool]:
    expected = json.loads((FIXTURES / "EXPECTED.json").read_text())
    rows = []
    ok = True
    for name in sorted(expected):
        want = expected[name]
        ttl = (FIXTURES / f"{name}.ttl").read_text()
        got = evaluate(ttl, want["tier"], name)
        if want.get("undetermined"):
            actual_pass = got["undetermined"] and not got["pass"]
        elif want["tier"] == "advisory":
            actual_pass = (not got["undetermined"]
                           and len(got["violations"]) == want["advisory_count"])
        else:
            actual_pass = got["pass"] == want["pass"] and not got["undetermined"]
        if want["tier"] == "advisory" and got["violations"]:
            actual = f"{len(got['violations'])} warn"
        else:
            actual = "pass" if got["pass"] else "reject"
        good = actual_pass
        ok = ok and good
        rows.append(
            {
                "fixture": name,
                "tier": want["tier"],
                "expected": "pass" if want["pass"] else "reject",
                "expected_advisories": want["advisory_count"],
                "actual": actual,
                "actual_advisories": len(got["violations"]),
                "undetermined": got["undetermined"],
                "ok": good,
                "note": want["note"],
                "messages": [violation["message"] for violation in got["violations"]],
            }
        )
    return rows, ok


def run_replay() -> list[dict]:
    RESULTS.mkdir(exist_ok=True)
    out = RESULTS / "extracted"
    out.mkdir(exist_ok=True)
    rows = []
    for entry in load_manifest():
        feature = extract_artifact(entry)
        ttl = ttl_for(feature)
        (out / f"{feature['id']}.ttl").write_text(ttl)
        blocking = evaluate(ttl, "blocking", feature["id"])
        advisory = (
            evaluate(ttl, "advisory", feature["id"])
            if feature["kind"] == "prd"
            else {"violations": []}
        )
        rows.append(
            {
                "artifact": feature["id"],
                "task": entry.get("task"),
                "reason": entry.get("reason", ""),
                "blocking_pass": blocking["pass"],
                "blocking_messages": [v["message"] for v in blocking["violations"]],
                "advisory_count": len(advisory["violations"]),
                "advisory_messages": [v["message"] for v in advisory["violations"]],
            }
        )
    return rows


def write_markdown(fixtures: list[dict], replay: list[dict]) -> str:
    lines = ["# Gate results", ""]
    passed = sum(1 for row in fixtures if row["ok"])
    lines += [
        f"Fixtures: **{passed}/{len(fixtures)}** as expected.",
        "",
        "## Fixtures",
        "",
        "| fixture | tier | expected | actual | messages | ok |",
        "|---|---|---|---|---|---|",
    ]
    for row in fixtures:
        lines.append(
            f"| `{row['fixture']}` | {row['tier']} | {row['expected']} | "
            f"{row['actual']} | {row['actual_advisories']} | "
            f"{'yes' if row['ok'] else '**NO**'} |"
        )
    lines += ["", "## Corpus replay", "",
              "Every row is an artifact a human already approved at its checkpoint.",
              "",
              "| artifact | blocking | advisories | advisory messages |",
              "|---|---|---|---|"]
    for row in replay:
        advisories = "; ".join(m.split(":")[0] for m in row["advisory_messages"]) or "—"
        lines.append(
            f"| `{row['artifact']}` | "
            f"{'pass' if row['blocking_pass'] else '**reject**'} | "
            f"{row['advisory_count']} | {advisories} |"
        )
    blocking_failures = [row for row in replay if not row["blocking_pass"]]
    lines += [
        "",
        f"Blocking failures on approved artifacts: **{len(blocking_failures)}**.",
        "",
    ]
    return "\n".join(lines)


def main() -> int:
    fixture_rows, ok = run_fixtures()
    replay_rows = run_replay()
    RESULTS.mkdir(exist_ok=True)
    (RESULTS / "gate.json").write_text(
        json.dumps(
            {"fixtures": fixture_rows, "replay": replay_rows},
            indent=2,
            ensure_ascii=False,
        )
        + "\n"
    )
    report = write_markdown(fixture_rows, replay_rows)
    (RESULTS / "gate.md").write_text(report)
    print(report)
    failures = [row for row in fixture_rows if not row["ok"]]
    if failures:
        print("FIXTURE MISMATCHES:")
        for row in failures:
            print(f"  {row['fixture']}: expected {row['expected']}, got {row['actual']}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
