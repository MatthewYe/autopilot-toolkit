# Implementer ontology experiment

Deterministic experiment for converting `autopilot-implementer`'s constraints and
context selection into an ontology-backed layer. See `DESIGN.md` for the proposal,
decision tree and validation protocol.

## What is here

- `ontology/implementer.ttl`: 37 clauses, 10 rules, task/report/context classes.
- `data/tasks.ttl`: five real tickets from Spec #121 (`T122`–`T126`).
- `data/contexts.ttl`: 17 context cards with source pointers and excerpts.
- `data/gold.json`: hand-assigned gold clause/context sets.
- `shapes/report-shapes.ttl`: 10 SHACL shapes for the `IMPLEMENTER_REPORT` contract.
- `fixtures/`: 13 report fixtures (1 valid, 12 invalid).
- `corpus/`: GitHub issues #121–#126 and PR #127 diff.
- `scripts/`: artifact builder, selector, report builder, gate runner, summaries.
- `results/`: selection, injection, aggregate, gate, and rendered plan outputs (`plan-T*.md`).

## Run

```bash
cd experiments/ontology-implementer
export OPEN_ONTOLOGIES_BIN=/path/to/open-ontologies
# Context excerpts are read from the T126 base commit, not the working tree.
export AUTOPILOT_REPO=/path/to/autopilot-toolkit
export AUTOPILOT_BASE_COMMIT=98270ef
python3 scripts/build_artifacts.py
python3 scripts/select_ablation.py
python3 scripts/summarize.py
python3 scripts/injection_variants.py
python3 scripts/render_plan.py
python3 scripts/build_reports.py
python3 scripts/run_gate.py
```

The only external dependency is the pinned `open-ontologies` CLI v2.1.0
(`OPEN_ONTOLOGIES_BIN`, or the local default used by `scripts/lib.py`). No network
and no API key are required for the deterministic stages.

## Current results

Selection over T122–T126 (mean):

| arm | clause recall | clause precision | context recall | context precision | exact | hybrid est. | rendered |
|---|---|---|---|---|---|---|---|
| none | 0.00 | 0.00 | 0.00 | 0.00 | 0/5 | 0 | 540 |
| generic | 1.00 | 0.676 | 1.00 | 0.447 | 0/5 | 2,875 | 9,874 |
| keyword | 0.368 | 0.940 | 0.164 | 1.00 | 0/5 | 660 | 2,193 |
| ontology | 1.00 | 0.984 | 1.00 | 1.00 | 3/5 | 1,743 | 5,833 |

Report gate: 13/13 fixtures matched; valid exit 0; 12 invalid exit 1; 0 false rejects.

Phase 2 pilot (T126, 2 runs per arm, `deepseek-flash`): all four runs passed the
hidden-test overlay, structural check, and report contract. `ontology-1` needed a
serial rerun for a known `shared` `PROJECT_ROOT` test race. The result is null:
both arms implemented the contract and passed. This is 4 runs on one ticket, not
the pre-registered 30-run protocol.

## What this does and does not show

Shows:

- Conditions over real ticket facts select the applicable clauses and context cards.
- Ontology selection has much higher precision than "inject everything" and much
  higher recall than keyword matching, with 39.4% fewer characters than generic in the hybrid estimate; the actual
  rendered plans are 40.9% smaller than the generic payload.
- A SHACL gate can enforce the report/status/retry/spec contract deterministically.

Does not show:

- That a model follows the injected clauses (`selection != compliance`).
- That code quality or hidden-test pass rate improves.
- That the gate catches real code defects; it checks the report, not the code.
- That this is production-ready; no upstream repository was modified.

## Known gaps

- Phase 2 is a 4-run pilot, not the full 30-run protocol; `test_build` /
  `test_install` were not completed (see `REPLAY.md`).
- Five tickets, one spec, same author; gold clause sets share the same condition
  design, so clause recall is partly definitional. Context gold is independent.
- Gate fixtures are synthetic; Phase A in `DESIGN.md` uses real PR bodies.
- No merged "skill conversion" PR baseline exists; #127/#153/#154/#155 are task
  baselines, not conversion baselines.
