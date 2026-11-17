#!/usr/bin/env python3
"""Build the implementer ontology, task data, context data and gold sets."""
from __future__ import annotations
import json
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXPERIMENT = ROOT
REPO = Path(os.environ.get('AUTOPILOT_REPO', Path(__file__).resolve().parents[3]))


def esc(s: str) -> str:
    return (s.replace('\\', '\\\\').replace('"', '\\"')
             .replace('\n', '\\n').replace('\r', ''))


def lit(s: str) -> str:
    return '"' + esc(s) + '"'


def int_lit(n: int) -> str:
    return f'"{n}"^^<http://www.w3.org/2001/XMLSchema#integer>'


def excerpt(path: Path, start: int, end: int, limit: int = 900) -> str:
    lines = path.read_text(errors='replace').splitlines()
    text = '\n'.join(lines[start - 1:end])
    return text[:limit]


def issue_body(n: int) -> str:
    data = json.loads((EXPERIMENT / 'corpus' / f'issue-{n}.json').read_text())
    return data['body']


# ── clauses ────────────────────────────────────────────────────────────────
# id, axis, text, check, conditions, anchor
CLAUSES = [
    ('I01', 'TDD', 'No production code without a failing test first.', 'tdd_evidence', {}, 'SKILL.md Step 2 iron rule'),
    ('I02', 'TDD', 'Tests assert observable behavior, not internal structure.', 'test_review', {}, 'SKILL.md TDD'),
    ('I03', 'TDD', 'Mock only at system boundaries; never mock own types.', 'test_review', {}, 'SKILL.md TDD'),
    ('I04', 'TDD', 'Run RED, then GREEN, then REFACTOR; do not skip RED.', 'tdd_evidence', {}, 'SKILL.md TDD'),
    ('I05', 'TDD', 'Every acceptance criterion has at least one test.', 'ac_coverage', {}, 'SKILL.md Step 2'),
    ('I06', 'Context', 'Read the issue or AGENT-BRIEF before editing.', 'context_read', {}, 'SKILL.md Step 1'),
    ('I07', 'Context', 'Read GLOSSARY.md (fallback CONTEXT.md).', 'context_read', {}, 'SKILL.md Step 1'),
    ('I08', 'Context', 'Read the linked spec/PRD/ADR before editing.', 'context_read', {'whenSpecLinked': True}, 'SKILL.md Step 1'),
    ('I09', 'Context', 'Check Seam annotations and stay inside the named seam.', 'context_read', {}, 'SKILL.md Step 1'),
    ('I10', 'Context', 'Resolve CROSS_ISSUE_SUGGESTIONS in SUGGESTION_RESOLUTIONS.', 'suggestion_resolution', {'whenSuggestions': True}, 'SKILL.md Step 2.5'),
    ('I11', 'Retry', 'Retry mode: fix only the Critical findings (canonical).', 'retry_scope', {'whenRound': 'retry', 'whenRuntime': 'canonical'}, 'SKILL.md retry mode'),
    ('I12', 'Retry', 'Retry mode: fix all Critical and Important findings (DSH).', 'retry_scope', {'whenRound': 'retry', 'whenRuntime': 'dsh'}, 'dsh/SKILL.md retry mode'),
    ('I13', 'Retry', 'Retry mode: do not redo already-passed acceptance criteria.', 'retry_scope', {'whenRound': 'retry'}, 'SKILL.md retry mode'),
    ('I14', 'Retry', 'Retry mode: no new features.', 'retry_scope', {'whenRound': 'retry'}, 'SKILL.md retry mode'),
    ('I15', 'Refactoring', 'REFACTORING preserves behavior; skip RED for new code but run existing tests before and after.', 'refactoring_evidence', {'whenRefactoring': True}, 'SKILL.md REFACTORING'),
    ('I16', 'Diagnosis', 'After at most two diagnosis hypotheses, report BLOCKED instead of guessing.', 'diagnosis', {}, 'SKILL.md diagnosis'),
    ('I17', 'SelfReview', 'Self-review runs the full test suite.', 'self_review', {}, 'SKILL.md Step 2.5'),
    ('I18', 'SelfReview', 'Self-review checks acceptance-criteria coverage.', 'self_review', {}, 'SKILL.md Step 2.5'),
    ('I19', 'SelfReview', 'Self-review checks scope creep.', 'self_review', {}, 'SKILL.md Step 2.5'),
    ('I20', 'Report', 'CHANGED_FILES lists every contract-referenced file; mark unedited files (pre-existing).', 'changed_files', {}, 'SKILL.md report'),
    ('I21', 'Report', 'Report starts IMPLEMENTER_REPORT: and carries ROUND, STATUS, SUGGESTION_RESOLUTIONS, SELF_REVIEW, CHANGED_FILES, SUMMARY.', 'report_schema', {}, 'SKILL.md report'),
    ('I22', 'Report', 'DONE only if every acceptance criterion has executed passing evidence.', 'status_evidence', {}, 'SKILL.md status'),
    ('I23', 'Report', 'TOOLCHAIN: unavailable caps STATUS at UNVERIFIED.', 'status_evidence', {'whenToolchain': 'unavailable'}, 'SKILL.md status'),
    ('I24', 'Report', 'UNVERIFIED annotates each acceptance criterion with its verification method.', 'status_evidence', {'whenUnverified': True}, 'SKILL.md status'),
    ('I25', 'Diagnosis', 'BLOCKED names the blocking fact after two failed hypotheses.', 'diagnosis', {}, 'SKILL.md status'),
    ('I26', 'Diagnosis', 'NEEDS_CONTEXT names the missing fact; do not guess.', 'diagnosis', {}, 'SKILL.md status'),
    ('I27', 'Guardrail', 'Do not modify scope or acceptance criteria.', 'guardrail', {}, 'SKILL.md prohibited'),
    ('I28', 'Guardrail', 'Do not guess a fix; diagnose before editing.', 'guardrail', {}, 'SKILL.md prohibited'),
    ('I29', 'Quality', 'Run the type-check/lint for changed Rust code.', 'typecheck', {'whenChangeKind': 'rust-crate'}, 'SKILL.md Step 2'),
    ('I30', 'Context', 'Read sibling modules named in CHANGED_FILES.', 'context_read', {'whenMultiModule': True}, 'SKILL.md Step 1'),
    ('I31', 'Risk', 'For data-loss or rollback work, add a restore/rollback test.', 'risk_test', {'whenRisk': 'data-loss'}, 'SKILL.md scope'),
    ('I32', 'Risk', 'For API-contract changes, update every caller and doc.', 'risk_contract', {'whenRisk': 'api-contract'}, 'SKILL.md scope'),
    ('I33', 'Risk', 'For behavior-preservation work, keep existing outputs and list deliberate changes.', 'risk_contract', {'whenRisk': 'behavior-preservation'}, 'SKILL.md scope'),
    ('I34', 'Guardrail', 'Do not add undeclared dependencies.', 'guardrail', {}, 'SKILL.md prohibited'),
    ('I35', 'Runtime', 'DSH: use scripts/sandboxed-rust-script.sh and respect workspace-write.', 'runtime', {'whenRuntime': 'dsh'}, 'dsh/SKILL.md sandbox'),
    ('I36', 'Runtime', 'DSH: run scripts/refresh-rs-cache.sh after crates/* changes.', 'runtime', {'whenRuntime': 'dsh', 'whenChangeKind': 'rust-crate'}, 'dsh/SKILL.md sandbox'),
    ('I37', 'Spec', 'Spec-linked work maps each AC to the spec and records deliberate behavior changes.', 'spec_mapping', {'whenSpecLinked': True}, 'orchestrator spec contract'),
]

# ── rules with provenance ─────────────────────────────────────────────────
RULES = [
    ('R1', 'TDD gate', ['I01', 'I02', 'I03', 'I04', 'I05']),
    ('R2', 'Context gate', ['I06', 'I07', 'I08', 'I09', 'I10']),
    ('R3', 'Retry gate', ['I11', 'I12', 'I13', 'I14']),
    ('R4', 'Refactoring gate', ['I15']),
    ('R5', 'Diagnosis gate', ['I16', 'I25', 'I26', 'I28']),
    ('R6', 'Self-review gate', ['I17', 'I18', 'I19', 'I20']),
    ('R7', 'Report gate', ['I21', 'I22', 'I23', 'I24']),
    ('R8', 'Risk gate', ['I29', 'I30', 'I31', 'I32', 'I33', 'I34']),
    ('R9', 'Runtime gate', ['I35', 'I36']),
    ('R10', 'Spec gate', ['I08', 'I37']),
]

# ── tasks ─────────────────────────────────────────────────────────────────
# gold_contexts are explicit, hand-assigned from the ticket's AC and the
# files the PR actually touched.
TASKS = [
    dict(id='T122', issue=122, title='Expected-set entry resolves its skill files (expand)',
         changeKinds=['rust-crate'], risks=['api-contract', 'behavior-preservation'],
         specLinked=True, multiModule=False,
         goldContexts=['CTX_ISSUE_122', 'CTX_SPEC_121', 'CTX_ADR_0045', 'CTX_GLOSSARY',
                       'CTX_SKILL_INDEX', 'CTX_TESTS_SKILL_INDEX']),
    dict(id='T123', issue=123, title='Validation consumes enumerated skill files',
         changeKinds=['rust-crate'], risks=['api-contract', 'behavior-preservation'],
         specLinked=True, multiModule=True,
         goldContexts=['CTX_ISSUE_123', 'CTX_SPEC_121', 'CTX_ADR_0045', 'CTX_GLOSSARY',
                       'CTX_SKILL_INDEX', 'CTX_VALIDATION', 'CTX_TESTS_VALIDATION']),
    dict(id='T124', issue=124, title='Dev and pack consume skill files',
         changeKinds=['rust-crate'], risks=['data-loss', 'behavior-preservation'],
         specLinked=True, multiModule=True,
         goldContexts=['CTX_ISSUE_124', 'CTX_SPEC_121', 'CTX_ADR_0045', 'CTX_GLOSSARY',
                       'CTX_SKILL_INDEX', 'CTX_DEPLOY', 'CTX_TESTS_DEPLOY']),
    dict(id='T125', issue=125, title='Toolkit-setup reads skill files',
         changeKinds=['skill', 'test'], risks=['behavior-preservation'],
         specLinked=True, multiModule=True,
         goldContexts=['CTX_ISSUE_125', 'CTX_SPEC_121', 'CTX_ADR_0045', 'CTX_GLOSSARY',
                       'CTX_SKILL_INDEX', 'CTX_TOOLKIT_SETUP', 'CTX_TESTS_TOOLKIT_SETUP']),
    dict(id='T126', issue=126, title='Retire the entry-level resolution status',
         changeKinds=['rust-crate'], risks=['api-contract', 'behavior-preservation'],
         specLinked=True, multiModule=True,
         goldContexts=['CTX_ISSUE_126', 'CTX_SPEC_121', 'CTX_ADR_0045', 'CTX_GLOSSARY',
                       'CTX_SKILL_INDEX', 'CTX_VALIDATION', 'CTX_DEPLOY', 'CTX_PR_127',
                       'CTX_TESTS_ALL', 'CTX_TESTS_DEPLOY', 'CTX_TESTS_SKILL_INDEX',
                       'CTX_TESTS_VALIDATION']),
]

BASE_CLAUSES = ['I01', 'I02', 'I03', 'I04', 'I05', 'I06', 'I07', 'I09', 'I16',
                'I17', 'I18', 'I19', 'I20', 'I21', 'I22', 'I25', 'I26', 'I27',
                'I28', 'I34']


def clause_matches(c: tuple, task: dict) -> bool:
    conditions = c[4]
    for key, want in conditions.items():
        if key == 'whenSpecLinked':
            if task.get('specLinked', False) != want:
                return False
        elif key == 'whenMultiModule':
            if task.get('multiModule', False) != want:
                return False
        elif key == 'whenUnverified':
            if task.get('unverified', False) != want:
                return False
        elif key == 'whenRefactoring':
            if task.get('refactoring', False) != want:
                return False
        elif key == 'whenSuggestions':
            if task.get('suggestions', False) != want:
                return False
        elif key == 'whenChangeKind':
            if want not in task.get('changeKinds', []):
                return False
        elif key == 'whenRisk':
            if want not in task.get('risks', []):
                return False
        elif key == 'whenRuntime':
            if task.get('runtime', 'canonical') != want:
                return False
        elif key == 'whenRound':
            if task.get('round', 'first') != want:
                return False
        elif key == 'whenToolchain':
            if task.get('toolchain', 'available') != want:
                return False
        else:
            raise ValueError(key)
    return True


def gold_clauses(task: dict) -> list[str]:
    out = [c[0] for c in CLAUSES if clause_matches(c, task)]
    # Hand-checked exceptions where the conditional model is broader than the
    # ticket's actual obligation (kept explicit so the experiment can miss).
    if task['id'] == 'T122' and 'I33' in out:
        out.remove('I33')  # additive interface change; no prior behavior to preserve
    if task['id'] == 'T124' and 'I31' in out:
        out.remove('I31')  # packaging gate, no data mutation/rollback path
    return out


# ── contexts ──────────────────────────────────────────────────────────────
def ctx(id_, title, path, start, end, conditions, limit=900):
    text = excerpt(REPO / path, start, end, limit) if path else ''
    return dict(id=id_, title=title, path=str(path) if path else '', start=start,
                end=end, conditions=conditions, excerpt=text, charCount=len(text))


CONTEXTS = []
for t in TASKS:
    n = t['issue']
    body = issue_body(n)
    body = body[:1200]
    CONTEXTS.append(dict(id=f'CTX_ISSUE_{n}', title=f'Issue #{n}', path=f'corpus/issue-{n}.json',
                         start=1, end=1, conditions={'forTask': f'T{t["id"][1:]}'},
                         excerpt=body, charCount=len(body)))

spec = issue_body(121)[:1600]
CONTEXTS.append(dict(id='CTX_SPEC_121', title='Spec #121', path='corpus/issue-121.json',
                     start=1, end=1, conditions={'whenSpecLinked': True}, excerpt=spec, charCount=len(spec)))
CONTEXTS.append(ctx('CTX_ADR_0045', 'ADR 0045', 'docs/adr/0045-expected-set-entry-resolves-its-skill-files.md', 1, 40, {'whenSpecLinked': True}, 1100))
CONTEXTS.append(ctx('CTX_GLOSSARY', 'GLOSSARY — Skill file', 'GLOSSARY.md', 14, 26, {}, 700))
CONTEXTS.append(ctx('CTX_SKILL_INDEX', 'skill-index interface', 'crates/skill-index/src/lib.rs', 25, 100, {'forTask': ['T122', 'T123', 'T124', 'T125', 'T126']}, 1000))
CONTEXTS.append(ctx('CTX_TESTS_SKILL_INDEX', 'skill-index enumerator tests', 'crates/skill-index/src/lib.rs', 769, 950, {'forTask': ['T122', 'T126']}, 900))
CONTEXTS.append(ctx('CTX_VALIDATION', 'validation-runner targets', 'crates/validation-runner/src/lib.rs', 40, 95, {'forTask': ['T123', 'T126']}, 900))
CONTEXTS.append(ctx('CTX_TESTS_VALIDATION', 'validation pipeline tests', 'crates/validation-runner/src/lib.rs', 1225, 1465, {'forTask': ['T123', 'T126']}, 900))
CONTEXTS.append(ctx('CTX_DEPLOY', 'deploy dev/pack strictness', 'crates/deploy/src/pack.rs', 181, 220, {'forTask': ['T124', 'T126']}, 800))
CONTEXTS.append(ctx('CTX_TESTS_DEPLOY', 'deploy tests', 'crates/deploy/src/pack.rs', 342, 370, {'forTask': ['T124', 'T126']}, 800))
CONTEXTS.append(ctx('CTX_TOOLKIT_SETUP', 'toolkit-setup procedure', 'skills/autopilot/toolkit-setup/SKILL.md', 50, 80, {'forTask': ['T125']}, 900))
CONTEXTS.append(ctx('CTX_TESTS_TOOLKIT_SETUP', 'toolkit-setup suite', 'tests/test_toolkit_setup.rs', 60, 120, {'forTask': ['T125']}, 800))
pr_body = (EXPERIMENT / 'corpus' / 'pr-127.diff').read_text()[:1200]
CONTEXTS.append(dict(id='CTX_PR_127', title='PR #127 deliberate changes', path='corpus/pr-127.diff',
                     start=1, end=1, conditions={'forTask': ['T126']}, excerpt=pr_body, charCount=len(pr_body)))
CONTEXTS.append(ctx('CTX_TESTS_ALL', 'workspace test list', 'tests/test_toolkit_setup.rs', 1, 60, {'forTask': ['T126']}, 700))

# ── emit TTL ──────────────────────────────────────────────────────────────
HEADER = '''@prefix ar: <https://example.org/autopilot/implementer#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
'''


def build_ontology() -> str:
    out = [HEADER]
    for cls, parent in [('Clause', 'owl:Thing'), ('Guardrail', 'ar:Clause'),
                        ('ReportContract', 'ar:Clause'), ('JudgementRule', 'owl:Thing'),
                        ('Task', 'owl:Thing'), ('ContextSource', 'owl:Thing'),
                        ('ImplementationPlan', 'owl:Thing'), ('Report', 'owl:Thing'),
                        ('Check', 'owl:Thing'), ('Status', 'owl:Thing'),
                        ('AcResult', 'owl:Thing'), ('ChangedFile', 'owl:Thing')]:
        out.append(f'ar:{cls} a owl:Class ; rdfs:subClassOf {parent} .')
    props = ['clauseId', 'axis', 'text', 'sourceAnchor', 'keyword', 'defaultCheck',
             'whenSource', 'whenRuntime', 'whenSpecLinked', 'whenMultiModule',
             'whenUnverified', 'whenRound', 'whenRefactoring', 'whenSuggestions',
             'whenToolchain', 'whenRisk', 'whenChangeKind', 'scope', 'forTask',
             'injectsClause', 'injectsContext', 'hasClause', 'hasContext',
             'taskId', 'title', 'source', 'runtime', 'changeKind', 'risk',
             'goldClause', 'goldContext', 'charCount', 'excerpt', 'sourcePath',
             'sourceStart', 'sourceEnd', 'hasCheck', 'checkClause', 'status',
             'round', 'toolchain', 'acStatus', 'hasAc', 'verificationMethod',
             'changedFile', 'changedKind', 'tddEvidence', 'selfReview',
             'hypotheses', 'missingContext', 'retryScope', 'summary', 'specLinked',
             'multiModule', 'refactoring', 'suggestions', 'unverified']
    for p in props:
        out.append(f'ar:{p} a owl:ObjectProperty .')
    for c in CLAUSES:
        cid, axis, text, check, conditions, anchor = c
        out.append(f'''ar:{cid} a ar:Guardrail ;
  ar:clauseId {lit(cid)} ;
  ar:axis {lit(axis)} ;
  ar:text {lit(text)} ;
  ar:defaultCheck {lit(check)} ;
  ar:sourceAnchor {lit(anchor)} ;
  ar:scope {lit('ReviewScope')} .''')
        for k, v in conditions.items():
            out.append(f'ar:{cid} ar:{k} {lit(str(v).lower()) if isinstance(v, bool) else lit(v)} .')
    for rid, label, cites in RULES:
        out.append(f'ar:{rid} a ar:JudgementRule ; ar:clauseId {lit(rid)} ; ar:text {lit(label)} .')
        for cid in cites:
            out.append(f'ar:{rid} ar:citesClause ar:{cid} .')
    return '\n'.join(out) + '\n'


def build_tasks() -> str:
    out = [HEADER]
    for t in TASKS:
        out.append(f'''ar:{t['id']} a ar:Task ;
  ar:taskId {lit(t['id'])} ;
  ar:title {lit(t['title'])} ;
  ar:source {lit('github')} ;
  ar:runtime {lit(t.get('runtime', 'canonical'))} ;
  ar:specLinked {str(t['specLinked']).lower()} ;
  ar:multiModule {str(t['multiModule']).lower()} ;
  ar:refactoring {str(t.get('refactoring', False)).lower()} ;
  ar:suggestions {str(t.get('suggestions', False)).lower()} ;
  ar:unverified {str(t.get('unverified', False)).lower()} ;
  ar:round {lit(t.get('round', 'first'))} ;
  ar:toolchain {lit(t.get('toolchain', 'available'))} .''')
        for k in t['changeKinds']:
            out.append(f'ar:{t["id"]} ar:changeKind {lit(k)} .')
        for r in t['risks']:
            out.append(f'ar:{t["id"]} ar:risk {lit(r)} .')
        for cid in gold_clauses(t):
            out.append(f'ar:{t["id"]} ar:goldClause ar:{cid} .')
        for cid in t['goldContexts']:
            out.append(f'ar:{t["id"]} ar:goldContext ar:{cid} .')
    return '\n'.join(out) + '\n'


def build_contexts() -> str:
    out = [HEADER]
    for c in CONTEXTS:
        cid = c['id']
        out.append(f'''ar:{cid} a ar:ContextSource ;
  ar:clauseId {lit(cid)} ;
  ar:title {lit(c['title'])} ;
  ar:sourcePath {lit(c['path'])} ;
  ar:sourceStart {int_lit(c['start'])} ;
  ar:sourceEnd {int_lit(c['end'])} ;
  ar:charCount {int_lit(c['charCount'])} ;
  ar:excerpt {lit(c['excerpt'])} .''')
        for k, v in c['conditions'].items():
            vals = v if isinstance(v, list) else [v]
            for val in vals:
                out.append(f'ar:{cid} ar:{k} {lit(str(val).lower()) if isinstance(val, bool) else lit(val)} .')
    return '\n'.join(out) + '\n'


def main():
    (EXPERIMENT / 'ontology').mkdir(exist_ok=True)
    (EXPERIMENT / 'data').mkdir(exist_ok=True)
    (EXPERIMENT / 'ontology' / 'implementer.ttl').write_text(build_ontology())
    (EXPERIMENT / 'data' / 'tasks.ttl').write_text(build_tasks())
    (EXPERIMENT / 'data' / 'contexts.ttl').write_text(build_contexts())
    gold = {
        t['id']: {'clauses': gold_clauses(t), 'contexts': t['goldContexts']}
        for t in TASKS
    }
    (EXPERIMENT / 'data' / 'gold.json').write_text(json.dumps(gold, indent=2) + '\n')
    print('clauses', len(CLAUSES), 'rules', len(RULES), 'tasks', len(TASKS), 'contexts', len(CONTEXTS))
    for t in TASKS:
        print(t['id'], 'gold clauses', len(gold[t['id']]['clauses']), 'contexts', len(t['goldContexts']))


if __name__ == '__main__':
    main()
