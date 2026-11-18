"""Thin wrapper around open-ontologies for the Distill experiment."""
from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PREFIX = "https://example.org/autopilot/distill#"


def find_binary() -> Path:
    if os.environ.get("OPEN_ONTOLOGIES_BIN"):
        return Path(os.environ["OPEN_ONTOLOGIES_BIN"])
    found = shutil.which("open-ontologies")
    if found:
        return Path(found)
    raise RuntimeError(
        "open-ontologies binary not found; set OPEN_ONTOLOGIES_BIN to the "
        "open-ontologies CLI built by the ontology-driven-agents prototype"
    )


def _unquote(value: str) -> str:
    value = str(value).strip()
    if value.startswith("<") and value.endswith(">"):
        return value[1:-1]
    match = re.match(r'^"(.*?)"(?:\^\^.*)?$', value, re.S)
    return match.group(1) if match else value


def short(iri: str) -> str:
    return str(iri).rsplit("#", 1)[-1]


class Store:
    def __init__(self, files: list[Path]):
        self._tmp = tempfile.TemporaryDirectory(prefix="distill-gate-")
        self.path = Path(self._tmp.name)
        for file in files:
            self.load(file)

    def close(self) -> None:
        self._tmp.cleanup()

    def _run(self, args: list[str]) -> subprocess.CompletedProcess[str]:
        env = dict(os.environ)
        env["OPEN_ONTOLOGIES_STORAGE_MODE"] = "persistent"
        return subprocess.run(
            [str(find_binary()), "--no-connect", "--data-dir", str(self.path), *args],
            check=False,
            capture_output=True,
            text=True,
            timeout=180,
            env=env,
        )

    def load(self, path: Path) -> None:
        cp = self._run(["load", str(path)])
        if cp.returncode != 0:
            raise RuntimeError(f"load {path} failed: {cp.stderr.strip()}")

    def query(self, sparql: str) -> list[dict[str, str]]:
        cp = self._run(["query", sparql])
        if cp.returncode != 0:
            raise RuntimeError(f"query failed: {cp.stderr.strip()}")
        payload = json.loads(cp.stdout)
        return [{k: _unquote(v) for k, v in row.items()} for row in payload.get("results") or []]

    def shacl(self, shapes: Path) -> dict:
        cp = self._run(["shacl", str(shapes)])
        payload = json.loads(cp.stdout) if cp.stdout.strip() else {}
        return {
            "exit_code": cp.returncode,
            "conforms": payload.get("conforms"),
            "violations": payload.get("violations") or [],
            "stderr": cp.stderr,
        }


# Fact-valued properties whose objects are compared to a task's values.
FACT_PROPERTIES = {
    "whenStage": "stages",
    "whenChangeKind": "changeKinds",
    "whenSurface": "surfaces",
    "whenRuntime": "runtimes",
    "whenAmbiguity": "ambiguity",
}


def _literal(value: str) -> str:
    return value.lower() in {"true", "false"} and value.lower() in {"true", "false"}


def load_inventory(store: Store):
    store.load(ROOT / "ontology" / "distill.ttl")
    store.load(ROOT / "data" / "tasks.ttl")
    store.load(ROOT / "data" / "contexts.ttl")

    clauses: dict[str, dict] = {}
    for row in store.query(
        f"""PREFIX d: <{PREFIX}>
SELECT ?c ?id ?axis ?stage ?text ?check ?anchor ?keyword ?enforced WHERE {{
  ?c d:clauseId ?id ; d:axis ?axis ; d:stage ?stage ; d:text ?text ;
     d:defaultCheck ?check ; d:sourceAnchor ?anchor ; d:keyword ?keyword .
  OPTIONAL {{ ?c d:enforcedBy ?enforced }}
}}"""
    ):
        clauses[row["id"]] = {
            "iri": row["c"],
            "id": row["id"],
            "axis": short(row["axis"]).removeprefix("Axis"),
            "stage": short(row["stage"]),
            "text": row["text"],
            "check": short(row["check"]),
            "anchor": row["anchor"],
            "keyword": row["keyword"],
            "enforced": short(row["enforced"]) if row.get("enforced") else None,
            "conditions": {},
        }

    for row in store.query(
        f"""PREFIX d: <{PREFIX}>
SELECT ?id ?p ?v WHERE {{
  ?c d:clauseId ?id .
  ?c ?p ?v .
  FILTER(STRSTARTS(STR(?p), "https://example.org/autopilot/distill#when"))
}}"""
    ):
        prop = short(row["p"])
        value = row["v"]
        key = prop[4].lower() + prop[5:]
        clauses[row["id"]]["conditions"].setdefault(key, []).append(value)

    contexts: dict[str, dict] = {}
    for row in store.query(
        f"""PREFIX d: <{PREFIX}>
SELECT ?x ?id ?title ?path ?start ?end ?excerpt ?introduced WHERE {{
  ?x d:contextId ?id ; d:title ?title ; d:path ?path ;
     d:startLine ?start ; d:endLine ?end ; d:excerpt ?excerpt ;
     d:introducedAt ?introduced .
}}"""
    ):
        contexts[row["id"]] = {
            "iri": row["x"],
            "id": row["id"],
            "title": row["title"],
            "path": row["path"],
            "start": int(row["start"]),
            "end": int(row["end"]),
            "excerpt": row["excerpt"],
            "introducedAt": row["introduced"],
            "stages": [],
            "conditions": {},
        }

    for row in store.query(
        f"""PREFIX d: <{PREFIX}>
SELECT ?id ?stage WHERE {{
  ?x d:contextId ?id ; d:stage ?stage .
}}"""
    ):
        contexts[row["id"]]["stages"].append(short(row["stage"]))

    for row in store.query(
        f"""PREFIX d: <{PREFIX}>
SELECT ?id ?p ?v WHERE {{
  ?x d:contextId ?id .
  ?x ?p ?v .
  FILTER(STRSTARTS(STR(?p), "https://example.org/autopilot/distill#when"))
}}"""
    ):
        prop = short(row["p"])
        value = row["v"]
        key = prop[4].lower() + prop[5:]
        contexts[row["id"]]["conditions"].setdefault(key, []).append(value)

    tasks: dict[str, dict] = {}
    task_props = {
        "taskId": "id",
        "title": "title",
        "requirement": "requirement",
        "sourceAnchor": "sourceAnchor",
        "stage": "stage",
        "changeKind": "changeKinds",
        "surface": "surfaces",
        "runtime": "runtimes",
        "multiModule": "multiModule",
        "crossRuntime": "crossRuntime",
        "multiSlice": "multiSlice",
        "ambiguity": "ambiguity",
        "domainDocs": "domainDocs",
        "adr": "adr",
        "newAdr": "newAdr",
        "externalTracker": "externalTracker",
        "wideRefactor": "wideRefactor",
        "userFacing": "userFacing",
        "hasExistingSeam": "hasExistingSeam",
        "asOf": "asOf",
        "targetArtifact": "targetArtifact",
    }
    for row in store.query(
        f"""PREFIX d: <{PREFIX}>
SELECT ?t ?p ?v WHERE {{
  ?t a d:Task .
  ?t ?p ?v .
}}"""
    ):
        prop = short(row["p"])
        if prop not in task_props:
            continue
        task = tasks.setdefault(row["t"], {"iri": row["t"]})
        key = task_props[prop]
        value = row["v"]
        if key in {"id", "title", "requirement", "sourceAnchor", "ambiguity",
                   "asOf", "targetArtifact"}:
            task[key] = short(value) if prop in {"ambiguity"} else value
        elif key == "stage":
            task.setdefault("stages", []).append(short(value))
        elif key in {"multiModule", "crossRuntime", "multiSlice", "domainDocs", "adr",
                     "newAdr", "externalTracker", "wideRefactor", "userFacing",
                     "hasExistingSeam"}:
            task[key] = value.lower() == "true"
        else:
            task.setdefault(key, [])
            item = short(value)
            if item != "None":  # sentinel for a known-empty set
                task[key].append(item)
    for task in tasks.values():
        task.setdefault("stages", [])
    return clauses, tasks, contexts


def matches(conditions: dict, task: dict, stage: str) -> bool:
    """A clause/context applies when every declared condition matches the task."""
    for key, values in conditions.items():
        if key == "stage":
            if short(stage) not in {short(v) for v in values}:
                return False
            continue
        if key == "ambiguity":
            if task.get("ambiguity") is None:  # not yet collected: inject
                continue
            if task["ambiguity"] not in {short(v) for v in values}:
                return False
            continue
        if key in {"changeKind", "surface", "runtime"}:
            task_key = {"changeKind": "changeKinds", "surface": "surfaces",
                        "runtime": "runtimes"}[key]
            if task_key not in task:  # unknown fact: inject conservatively
                continue
            if not ({short(v) for v in values} & set(task[task_key])):
                return False
            continue
        if key in {"multiModule", "crossRuntime", "multiSlice", "domainDocs", "adr",
                   "newAdr", "externalTracker", "wideRefactor", "userFacing",
                   "hasExistingSeam"}:
            if key not in task:  # unknown fact: inject conservatively
                continue
            if (values[0].lower() == "true") != bool(task[key]):
                return False
            continue
        raise ValueError(f"unknown condition {key}")
    return True


def eligible_contexts(contexts: dict, task: dict) -> dict:
    """Drop the task's own target artifact before any arm selects contexts.

    A replay that can read the accepted answer is not a replay.  The filter
    lives here, not in the ontology arm, so ``stage`` and ``generic`` pay the
    same price.
    """
    target = task.get("targetArtifact")
    return {cid: card for cid, card in contexts.items() if card["path"] != target}


def historical(contexts: dict, ids: list[str], task: dict) -> list[str]:
    """Ground truth as it could have been referenced on the task date.

    The main gold set is the minimal useful pack for a current re-run; some of
    those documents postdate the historical artifact.  This subset measures
    that anachronism separately instead of letting it inflate recall.
    """
    as_of = task.get("asOf", "9999-12-31")
    return [cid for cid in ids if contexts[cid]["introducedAt"][:10] <= as_of]


def select(clauses: dict, contexts: dict, task: dict, stage: str):
    """The ontology arm: stage + declared task conditions, runner rules excluded."""
    selected_clauses = [
        clause
        for clause in clauses.values()
        if clause["stage"] == stage and clause["enforced"] is None
        and matches(clause["conditions"], task, stage)
    ]
    selected_contexts = [
        context
        for context in contexts.values()
        if (stage in context.get("stages", []) or "Cross" in context.get("stages", []))
        and matches(context["conditions"], task, stage)
    ]
    return (sorted(selected_clauses, key=lambda c: c["id"]),
            sorted(selected_contexts, key=lambda c: c["id"]))


def metrics(selected: list[str], gold: list[str]) -> tuple[float, float, int, int, int]:
    selected_set, gold_set = set(selected), set(gold)
    hit = len(selected_set & gold_set)
    recall = hit / len(gold_set) if gold_set else 1.0
    precision = hit / len(selected_set) if selected_set else (1.0 if not gold_set else 0.0)
    return recall, precision, hit, len(selected_set), len(gold_set)


def chars(clauses, contexts, mode: str) -> int:
    """Payload size under four rendering conventions.

    full    - clause text plus context excerpt (the actual injection payload)
    ids     - IDs only (cheapest possible pack)
    compact - clause IDs plus context title/path
    hybrid  - clause text plus context pointer only (no excerpt)
    """
    clause_text = sum(len(c["text"]) + len(c["id"]) + 6 for c in clauses)
    context_excerpt = sum(len(x["excerpt"]) + len(x["path"]) + 12 for x in contexts)
    context_pointer = sum(len(x["path"]) + len(str(x["start"])) + len(str(x["end"])) + 8
                          for x in contexts)
    ids = sum(len(c["id"]) + 1 for c in clauses) + sum(len(x["id"]) + 1 for x in contexts)
    if mode == "full":
        return clause_text + context_excerpt
    if mode == "ids":
        return ids
    if mode == "compact":
        return sum(len(c["id"]) + 1 for c in clauses) + context_pointer
    if mode == "hybrid":
        return clause_text + context_pointer
    raise ValueError(mode)


def estimate_tokens(char_count: int) -> int:
    """Rough English-token estimate; used only for relative payload comparison."""
    return (char_count + 3) // 4
