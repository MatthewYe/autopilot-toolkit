#!/usr/bin/env python3
"""Extract the facts the gate needs from a Distill draft.

The gate never reads prose semantics.  It reads a small feature graph:

* a PRD markdown file becomes one ``d:PrdDraft`` with structural counts
  (sections, stories, testing seam, out-of-scope items, file paths, code
  fences) and the conditional facts the gate needs (runtime plan, migration
  note, validation command, CI trigger);
* a set of published issues becomes one ``d:IssueSet`` with one ``d:Issue``
  per issue, parent presence, acceptance-criteria count and the blocked-by
  graph checks (dangling references, cycles).

Those facts are what a text check can decide without pretending to judge
prose.  Everything else stays with the human checkpoint.
"""
from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PREFIX = "https://example.org/autopilot/distill#"

REQUIRED_SECTIONS = (
    "Problem Statement",
    "Solution",
    "User Stories",
    "Implementation Decisions",
    "Testing Decisions",
    "Out of Scope",
)

RUN_TIME_WORDS = ("reasonix", "codex", "kimi", "dsh", "claude")
STORY_RE = re.compile(r"^\s*(?:\d+\.|[-*])\s+(.+)$")
FILE_RE = re.compile(
    r"`?[\w./-]+\.(?:rs|toml|json|sh|md|ya?ml|ts|tsx|js|py|lock)`?"
)
VALIDATION_WORDS = (
    "validation/run.rs",
    "scripts/check.rs",
    "cargo test",
    "npm test",
    "pytest",
    "test suite",
)
CI_WORDS = (
    ".github/workflows",
    "workflow",
    "weekly",
    "schedule",
    "cron",
    "ci ",
)


class DraftError(ValueError):
    """A draft cannot be extracted at all (missing file, bad JSON)."""


def _sections(text: str) -> dict[str, str]:
    """Map level-2 headings to their bodies (level-3 stays inside the body)."""
    sections: dict[str, str] = {}
    current: str | None = None
    body: list[str] = []
    for line in text.splitlines():
        match = re.match(r"^##\s+(.+?)\s*$", line)
        if match:
            if current is not None:
                sections[current] = "\n".join(body).strip()
            current = match.group(1).strip()
            body = []
        elif current is not None:
            body.append(line)
    if current is not None:
        sections[current] = "\n".join(body).strip()
    return sections


def _story_facts(body: str) -> tuple[int, int, int]:
    stories = [m.group(1).strip() for m in map(STORY_RE.match, body.splitlines()) if m]
    numbered = sum(1 for line in body.splitlines() if re.match(r"^\s*\d+\.\s+", line))
    well_formed = 0
    for story in stories:
        low = story.lower()
        if "as a" in low and "i want" in low and ("so that" in low or "so i can" in low):
            well_formed += 1
    return len(stories), numbered, well_formed


def extract_prd(path: Path, facts: dict, artifact_id: str, source: str) -> dict:
    """Turn one spec markdown file into a PRD feature dict."""
    return extract_prd_text(path.read_text(), facts, artifact_id, source)


def extract_prd_text(text: str, facts: dict, artifact_id: str, source: str) -> dict:
    """Turn spec markdown into a PRD feature dict (shared with fixtures)."""
    sections = _sections(text)
    stories, numbered, well_formed = _story_facts(sections.get("User Stories", ""))

    impl = sections.get("Implementation Decisions", "")
    testing = sections.get("Testing Decisions", "").lower()
    out_of_scope = sections.get("Out of Scope", "")
    out_items = len(
        [m for m in map(STORY_RE.match, out_of_scope.splitlines()) if m]
    )

    runtime_mentions = len(
        {word for word in RUN_TIME_WORDS if re.search(rf"\b{word}\b", text, re.I)}
    )
    lowered = text.lower()
    runtime_plan = runtime_mentions >= 2 and "fallback" in lowered
    empty_sections = sorted(name for name in sections if not sections[name].strip())
    return {
        "kind": "prd",
        "id": artifact_id,
        "source": source,
        "sections": sorted(sections),
        "empty_sections": empty_sections,
        "problemBodyChars": len(sections.get("Problem Statement", "")),
        "solutionBodyChars": len(sections.get("Solution", "")),
        "storyCount": stories,
        "numberedStoryCount": numbered,
        "wellFormedStoryCount": well_formed,
        "hasTestingSeam": "seam" in testing,
        "outOfScopeItemCount": out_items,
        "codeFenceCount": impl.count("```") // 2,
        "filePathCount": len(FILE_RE.findall(impl)),
        "runtimeMentions": runtime_mentions,
        "runtimePlan": runtime_plan,
        "mentionsMigration": bool(
            re.search(r"migrat|deprecat|supersed", lowered)
        ),
        "mentionsValidationCommand": any(word in lowered for word in VALIDATION_WORDS),
        "mentionsCiTrigger": any(word in lowered for word in CI_WORDS),
        "crossRuntime": bool(facts.get("crossRuntime", False)),
        "contractChange": bool(facts.get("contractChange", False)),
        "skillsChange": bool(facts.get("skillsChange", False)),
        "ciChange": bool(facts.get("ciChange", False)),
    }


def _issue_sections(body: str) -> dict[str, str]:
    sections: dict[str, str] = {}
    current: str | None = None
    chunk: list[str] = []
    for line in body.splitlines():
        match = re.match(r"^##\s+(.+?)\s*$", line)
        if match:
            if current is not None:
                sections[current] = "\n".join(chunk).strip()
            current = match.group(1).strip()
            chunk = []
        elif current is not None:
            chunk.append(line)
    if current is not None:
        sections[current] = "\n".join(chunk).strip()
    return sections


def _blocked_numbers(text: str) -> list[int]:
    return sorted({int(n) for n in re.findall(r"#(\d+)", text)})


def _has_cycle(edges: dict[int, list[int]]) -> bool:
    WHITE, GREY, BLACK = 0, 1, 2
    colour = {node: WHITE for node in edges}

    def visit(node: int) -> bool:
        colour[node] = GREY
        for nxt in edges.get(node, []):
            if colour.get(nxt, BLACK) == GREY:
                return True
            if colour.get(nxt, WHITE) == WHITE and visit(nxt):
                return True
        colour[node] = BLACK
        return False

    return any(colour[node] == WHITE and visit(node) for node in list(edges))


def extract_issues(paths: list[Path], facts: dict, artifact_id: str) -> dict:
    """Turn a set of published issues into an IssueSet feature dict."""
    payloads = [json.loads(path.read_text()) for path in paths]
    return extract_issues_from_payloads(
        payloads, facts, artifact_id, ", ".join(str(p) for p in paths))


def extract_issues_from_payloads(payloads: list[dict], facts: dict,
                                 artifact_id: str, source: str) -> dict:
    """Turn issue payloads into an IssueSet feature dict (shared with fixtures)."""
    issues = []
    numbers = set()
    bodies: dict[int, str] = {}
    for payload in payloads:
        number = int(payload["number"])
        numbers.add(number)
        bodies[number] = payload.get("body") or ""
    declared = []
    for number in sorted(numbers):
        sections = _issue_sections(bodies[number])
        parent = sections.get("Parent", "")
        criteria = sections.get("Acceptance criteria", "")
        ac_count = len(
            [m for m in map(STORY_RE.match, criteria.splitlines()) if m]
        )
        blocked = _blocked_numbers(sections.get("Blocked by", ""))
        # A heading with an empty body declares nothing; treat it like a
        # missing section so T03 blocks instead of assuming no edges.
        declared.append(bool(sections.get("Blocked by", "").strip()))
        issues.append(
            {
                "number": number,
                "hasParent": bool(re.search(r"#\d+|parent", parent, re.I)),
                "acCount": ac_count,
                "blockedBy": [n for n in blocked if n in numbers],
                "dangling": [n for n in blocked if n not in numbers],
                "hasBuild": bool(sections.get("What to build", "").strip()),
            }
        )
    dependencies_declared = all(declared)
    edges = {issue["number"]: issue["blockedBy"] for issue in issues}
    return {
        "kind": "issues",
        "id": artifact_id,
        "source": source,
        "issues": issues,
        "dependenciesDeclared": dependencies_declared,
        # A partial graph cannot prove acyclicity; the gate rejects undeclared
        # dependencies instead of treating "no section" as "no edges".
        "hasCycle": _has_cycle(edges) if dependencies_declared else False,
        "hasDanglingBlocker": any(issue["dangling"] for issue in issues),
        "multiSlice": bool(facts.get("multiSlice", False)),
    }


# ── TTL rendering ──────────────────────────────────────────────────────────
def _ttl_value(value) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    escaped = str(value).replace("\\", "\\\\").replace('"', '\\"')
    return f'"{escaped}"'


def prd_ttl(feature: dict) -> str:
    lines = [
        "@prefix d: <https://example.org/autopilot/distill#> .",
        "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .",
        "",
        f"d:{feature['id']} a d:PrdDraft ;",
    ]
    fields = []
    for section in feature["sections"]:
        fields.append(f'    d:section {_ttl_value(section)}')
    for section in feature["empty_sections"]:
        fields.append(f'    d:emptySection {_ttl_value(section)}')
    for key, value in feature.items():
        if key in {"kind", "id", "sections", "empty_sections"}:
            continue
        fields.append(f"    d:{key} {_ttl_value(value)}")
    lines.append(" ;\n".join(fields) + " .")
    return "\n".join(lines) + "\n"


def issues_ttl(feature: dict) -> str:
    lines = [
        "@prefix d: <https://example.org/autopilot/distill#> .",
        "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .",
        "",
        f"d:{feature['id']} a d:IssueSet ;",
        f"    d:dependenciesDeclared {_ttl_value(feature['dependenciesDeclared'])} ;",
        f"    d:hasCycle {_ttl_value(feature['hasCycle'])} ;",
        f"    d:hasDanglingBlocker {_ttl_value(feature['hasDanglingBlocker'])} ;",
        f"    d:multiSlice {_ttl_value(feature['multiSlice'])} ;",
        "    d:issue "
        + ", ".join(f"d:issue-{issue['number']}" for issue in feature["issues"])
        + " .",
        "",
    ]
    for issue in feature["issues"]:
        lines += [
            f"d:issue-{issue['number']} a d:Issue ;",
            f"    d:issueNumber {issue['number']} ;",
            f"    d:hasParent {_ttl_value(issue['hasParent'])} ;",
            f"    d:acCount {issue['acCount']} ;",
            f"    d:hasBuild {_ttl_value(issue['hasBuild'])} ;",
            f"    d:blockedBy "
            + (
                ", ".join(f"d:issue-{n}" for n in issue["blockedBy"])
                if issue["blockedBy"]
                else "()"
            )
            + " .",
            "",
        ]
    return "\n".join(lines)


def load_manifest(path: Path = ROOT / "data" / "corpus.json") -> list[dict]:
    return json.loads(path.read_text())["artifacts"]


def extract_artifact(entry: dict) -> dict:
    facts = entry.get("facts", {})
    if entry["kind"] == "prd":
        path = ROOT / entry["path"]
        return extract_prd(path, facts, entry["id"], entry["path"])
    if entry["kind"] == "issues":
        paths = [ROOT / p for p in entry["paths"]]
        return extract_issues(paths, facts, entry["id"])
    raise DraftError(f"unknown artifact kind {entry['kind']!r}")


def ttl_for(feature: dict) -> str:
    return prd_ttl(feature) if feature["kind"] == "prd" else issues_ttl(feature)


if __name__ == "__main__":
    for entry in load_manifest():
        printable = extract_artifact(entry)
        print(f"{printable['id']}: {json.dumps({k: v for k, v in printable.items() if k != 'issues'}, ensure_ascii=False)}")
