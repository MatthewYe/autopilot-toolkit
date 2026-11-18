"""Thin wrapper around open-ontologies for the implementer experiment."""
from __future__ import annotations
import json, os, re, shutil, subprocess, tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PREFIX = "https://example.org/autopilot/implementer#"


def find_binary() -> Path:
    if os.environ.get("OPEN_ONTOLOGIES_BIN"):
        return Path(os.environ["OPEN_ONTOLOGIES_BIN"])
    found = shutil.which("open-ontologies")
    if found:
        return Path(found)
    raise RuntimeError("open-ontologies binary not found; set OPEN_ONTOLOGIES_BIN")


def _unquote(value: str) -> str:
    value = str(value).strip()
    if value.startswith("<") and value.endswith(">"):
        return value[1:-1]
    m = re.match(r'^"(.*?)"(?:\^\^.*)?$', value, re.S)
    return m.group(1) if m else value


def short(iri: str) -> str:
    return str(iri).rsplit("#", 1)[-1]


class Store:
    def __init__(self, files: list[Path]):
        self._tmp = tempfile.TemporaryDirectory(prefix="implementer-gate-")
        self.path = Path(self._tmp.name)
        for f in files:
            self.load(f)

    def close(self):
        self._tmp.cleanup()

    def _run(self, args: list[str]) -> subprocess.CompletedProcess[str]:
        env = dict(os.environ)
        env["OPEN_ONTOLOGIES_STORAGE_MODE"] = "persistent"
        return subprocess.run([str(find_binary()), "--no-connect", "--data-dir", str(self.path), *args],
                              check=False, capture_output=True, text=True, timeout=180, env=env)

    def load(self, path: Path):
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
        return {"exit_code": cp.returncode, "conforms": payload.get("conforms"),
                "violations": payload.get("violations") or [],
                "stderr": cp.stderr}


def load_inventory(store: Store):
    store.load(ROOT / "ontology" / "implementer.ttl")
    store.load(ROOT / "data" / "tasks.ttl")
    store.load(ROOT / "data" / "contexts.ttl")

    clauses: dict[str, dict] = {}
    for row in store.query(f"""PREFIX ar: <{PREFIX}>
SELECT ?c ?id ?axis ?text ?check ?anchor WHERE {{
  ?c ar:clauseId ?id ; ar:axis ?axis ; ar:text ?text ;
     ar:defaultCheck ?check ; ar:sourceAnchor ?anchor .
}}"""):
        clauses[row["id"]] = {"iri": row["c"], "id": row["id"], "axis": row["axis"],
                              "text": row["text"], "check": row["check"],
                              "anchor": row["anchor"], "conditions": {}}
    for row in store.query(f"""PREFIX ar: <{PREFIX}>
SELECT ?id ?predicate ?value WHERE {{
  ?c a ar:Guardrail ; ar:clauseId ?id . ?c ?predicate ?value .
  FILTER(?predicate IN (ar:whenSource, ar:whenRuntime, ar:whenSpecLinked,
                        ar:whenMultiModule, ar:whenUnverified, ar:whenRound,
                        ar:whenRefactoring, ar:whenSuggestions, ar:whenToolchain,
                        ar:whenChangeKind, ar:whenRisk))
}}"""):
        clauses[row["id"]]["conditions"].setdefault(short(row["predicate"]), []).append(row["value"])

    tasks: dict[str, dict] = {}
    for row in store.query(f"""PREFIX ar: <{PREFIX}>
SELECT ?t ?id ?title ?source ?runtime ?base ?spec ?multi ?refactor ?suggestions ?unverified ?round ?toolchain WHERE {{
  ?t ar:taskId ?id ; ar:title ?title ; ar:source ?source ; ar:runtime ?runtime ; ar:baseCommit ?base ;
     ar:specLinked ?spec ; ar:multiModule ?multi ; ar:refactoring ?refactor ;
     ar:suggestions ?suggestions ; ar:unverified ?unverified ; ar:round ?round ;
     ar:toolchain ?toolchain .
}}"""):
        tasks[row["id"]] = {"iri": row["t"], "id": row["id"], "title": row["title"],
                            "source": row["source"], "runtime": row["runtime"], "base": row["base"],
                            "specLinked": row["spec"], "multiModule": row["multi"],
                            "refactoring": row["refactor"], "suggestions": row["suggestions"],
                            "unverified": row["unverified"], "round": row["round"],
                            "toolchain": row["toolchain"], "changeKinds": [], "risks": [],
                            "goldClauses": [], "goldContexts": []}
    for row in store.query(f"""PREFIX ar: <{PREFIX}>
SELECT ?id ?kind ?risk ?goldClause ?goldContext WHERE {{
  ?t ar:taskId ?id .
  OPTIONAL {{ ?t ar:changeKind ?kind }}
  OPTIONAL {{ ?t ar:risk ?risk }}
  OPTIONAL {{ ?t ar:goldClause ?goldClause }}
  OPTIONAL {{ ?t ar:goldContext ?goldContext }}
}}"""):
        task = tasks[row["id"]]
        for key, val in (("kind", "changeKinds"), ("risk", "risks"),
                         ("goldClause", "goldClauses"), ("goldContext", "goldContexts")):
            if row.get(key):
                v = short(row[key]) if key.startswith("gold") else row[key]
                if v not in task[val]:
                    task[val].append(v)

    contexts: dict[str, dict] = {}
    for row in store.query(f"""PREFIX ar: <{PREFIX}>
SELECT ?c ?id ?title ?path ?start ?end ?excerpt ?chars WHERE {{
  ?c a ar:ContextSource ; ar:clauseId ?id ; ar:title ?title ;
     ar:sourcePath ?path ; ar:sourceStart ?start ; ar:sourceEnd ?end ;
     ar:excerpt ?excerpt ; ar:charCount ?chars .
}}"""):
        contexts[row["id"]] = {"iri": row["c"], "id": row["id"], "title": row["title"],
                               "path": row["path"], "start": row["start"], "end": row["end"],
                               "excerpt": row["excerpt"],
                               "chars": int(row["chars"]), "conditions": {}}
    for row in store.query(f"""PREFIX ar: <{PREFIX}>
SELECT ?id ?predicate ?value WHERE {{
  ?c a ar:ContextSource ; ar:clauseId ?id . ?c ?predicate ?value .
  FILTER(?predicate IN (ar:whenSource, ar:whenRuntime, ar:whenChangeKind,
                        ar:whenRisk, ar:whenSpecLinked, ar:whenMultiModule, ar:forTask))
}}"""):
        contexts[row["id"]]["conditions"].setdefault(short(row["predicate"]), []).append(row["value"])
    return clauses, tasks, contexts


def condition_matches(conditions: dict[str, list[str]], task: dict) -> bool:
    """All condition families must match (AND); multiple values in one family are OR."""
    for predicate, values in conditions.items():
        values = set(values)
        if predicate == "forTask":
            matched = task["iri"] in values or task["id"] in values
        elif predicate == "whenSource":
            matched = task["source"] in values
        elif predicate == "whenRuntime":
            matched = task["runtime"] in values or task["runtime"].removeprefix("Runtime").lower() in values
        elif predicate == "whenSpecLinked":
            matched = task["specLinked"] in values
        elif predicate == "whenMultiModule":
            matched = task["multiModule"] in values
        elif predicate == "whenUnverified":
            matched = task["unverified"] in values
        elif predicate == "whenRefactoring":
            matched = task["refactoring"] in values
        elif predicate == "whenSuggestions":
            matched = task["suggestions"] in values
        elif predicate == "whenRound":
            matched = task["round"] in values
        elif predicate == "whenToolchain":
            matched = task["toolchain"] in values
        elif predicate == "whenChangeKind":
            matched = bool(values & set(task["changeKinds"]))
        elif predicate == "whenRisk":
            matched = bool(values & set(task["risks"]))
        else:
            matched = False
        if not matched:
            return False
    return True


def select(clauses, contexts, task):
    cs = [c for c in clauses.values() if condition_matches(c["conditions"], task)]
    xs = [c for c in contexts.values() if condition_matches(c["conditions"], task)]
    return sorted(cs, key=lambda c: c["id"]), sorted(xs, key=lambda c: c["id"])


def metrics(selected, gold):
    sel, g = set(selected), set(gold)
    hit = len(sel & g)
    recall = hit / len(g) if g else 0.0
    precision = hit / len(sel) if sel else 0.0
    return recall, precision, hit, len(sel), len(g)
