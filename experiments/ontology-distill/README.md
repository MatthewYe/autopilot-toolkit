# Distill ontology injection and gate

An offline experiment that asks whether the Distill workflow should stop
carrying its rules in prose and instead compose a stage context pack from an
ontology, then gate the draft before the human checkpoint.

The proposal and its grilling record are in [`DESIGN.md`](DESIGN.md) (Chinese).
The pre-registered ablation protocol, the executed offline results and the
unexecuted online design are in [`ABLATION.md`](ABLATION.md).

## What is here

| path | contents |
|---|---|
| `ontology/distill.ttl` | 45 clauses with stage, axis, check kind, source anchor and `when*` conditions; 8 are runner-enforced and never injected |
| `data/contexts.ttl` | 25 context cards: repo documents cut to a line range with a ≤360-char excerpt and stage/conditions |
| `data/tasks.ttl` | 8 real requirement tasks and their facts |
| `data/gold.json` | hand-labelled minimal clause/context set per task-stage |
| `data/corpus.json` | replay manifest: 4 accepted PRDs + one accepted issue set, with the facts and reason for each |
| `shapes/gate-shapes.ttl` | blocking structural invariants (sections, numbered stories, seam, out-of-scope, Parent, acceptance criteria, DAG cycles/dangling) |
| `shapes/advisory-shapes.ttl` | conditional rules that historical artifacts do not satisfy yet (file paths, runtime plan, migration, validation command, CI trigger, story-template drift) |
| `fixtures/` | 23 fixtures: single mutations of real artifacts, empty/short-body and dependency regressions, plus `EXPECTED.json` |
| `scripts/build_artifacts.py` | single source for the ontology, cards and tasks |
| `scripts/lib.py` | `open-ontologies` wrapper, environment matching and selection |
| `scripts/select_ablation.py` | five selection arms, metrics and rendered packs |
| `scripts/extract.py` | draft → feature graph (PRD markdown, published issue set) |
| `scripts/gate.py` | run one graph against a gate tier |
| `scripts/build_fixtures.py` | build and mutate the fixtures |
| `scripts/run_gate.py` | fixture unit test + corpus replay |
| `results/` | `selection.{md,json,txt}`, `gate.{md,json}`, `payloads/<task>-<stage>.md`, `extracted/*.ttl` |
| `corpus/` | pinned copies of the specs and issue JSON used for replay |

## Quick start

```bash
cd experiments/ontology-distill
export OPEN_ONTOLOGIES_BIN=/path/to/open-ontologies   # v2.1.0
python3 scripts/build_artifacts.py
python3 scripts/select_ablation.py
python3 scripts/build_fixtures.py
python3 scripts/run_gate.py
```

No network access, no model credentials. `run_gate.py` exits non-zero if any
fixture does not behave as `fixtures/EXPECTED.json` says.

## Results at a glance

Selection over 24 task-stages:

| arm | clause R | clause P | context R | context P | exact clauses | mean chars |
|---|---|---|---|---|---|---|
| none | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 433 |
| generic | 1.000 | 0.281 | 1.000 | 0.265 | 0.000 | 12401 |
| stage | 1.000 | 0.845 | 1.000 | 0.612 | 0.042 | 5447 |
| keyword | 0.095 | 0.476 | 0.402 | 0.615 | 0.000 | 1908 |
| ontology | 1.000 | 1.000 | 1.000 | 0.861 | 1.000 | 3976 |

`mean chars` is the actual `render_payload` length; every pack has a 433-char
fixed header, so the incremental clause+context payload drops from 5014 to
3543 (−29.3%), or −27.0% including the header. Each task's own target artifact
and its post-hoc decision documents (T6: ADR 0052 and the upstream-sync
procedure; T8: ADR 0045) are excluded from candidates in every arm, with the
latter also removed from the PRD gold. Exact context sets match in 10/24 packs
(0.417), reported separately because the clause set is the primary unit.

Clause precision 1.000 is **not** independent evidence: the gold labels and the
`when*` conditions were written by the same author from the same facts. The
comparison worth reading is the delta over the stage pack (characters −27.0% at
higher context precision); lexical retrieval's recall of 0.095 is the negative
control.

Gate: 23/23 fixtures as expected; replay of 5 human-approved artifacts has
**0 blocking failures**. Six rule checks (story form, file paths, runtime plan, migration note,
validation command, CI trigger) had to be demoted from blocking to advisory
after they rejected accepted PRDs; that deviation is recorded in `ABLATION.md`
§3.3.

## How it would integrate

Phase 1 (not implemented here) adds two sidecars and a checkpoint change:

1. `distill-context --stage <stage> --facts <json>` returns the `context_pack`
   at stage entry; a missing sidecar fails closed.
2. `distill-gate --stage <stage> --draft <file>` runs before the checkpoint;
   a violation prevents the checkpoint from being recorded.
3. checkpoint evidence stores `draft_sha256`, so a changed draft invalidates
   the approval that covered the old bytes.

Patch points: stage dispatch and `submit-evidence` in
`crates/distill-cli/src/main.rs`, checkpoint payload in
`crates/distill-cli/workflows/distill.v1.json`, evidence validation around
`crates/distill-cli/src/main.rs:888`. The upstream stage skills
(`grill-with-docs`, `to-spec`, `to-tickets`) stay unmodified, per ADR 0013.

## Limits

- Offline only: no end-to-end model runs, so the effect on artifact quality is
  unmeasured. The online protocol is pre-registered but not executed.
- Eight tasks from one repository, with complete artifacts available for
  replay; selection bias toward well-documented work.
- Single annotator for gold; no inter-annotator agreement.
- The gate checks structure, not meaning. A structurally complete but vague PRD
  still passes. Human review remains the semantic gate.
- The advisory tier is a backlog, not a pass: those conditions are real rules
  that the current artifacts miss.

## Provenance

Corpus dates and sources: PRD 0002 (2026-06-30), PRD 0003/0004 (2026-07-24),
spec #121 (2026-09-11), issues #122–#126 (2026-09-11), `docs/issues/19`–`20`
(2026-10-02, reviewed but not copied: they are Chinese implementation notes, not
to-spec specs, so they cannot be replayed through the PRD gate). The ontology and cards are generated from the unmodified
stage skills, the Distill workflow JSON and the repository ADRs; every clause
carries a source anchor.
