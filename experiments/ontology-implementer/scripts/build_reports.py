#!/usr/bin/env python3
"""Generate the implementer report fixtures used by the SHACL gate."""
from __future__ import annotations
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib import Store, load_inventory, select

ROOT = Path(__file__).resolve().parents[1]
P = 'https://example.org/autopilot/implementer#'
HEAD = f'''@prefix ar: <{P}> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
'''

# The plan is the ontology selector's output for T122; keeping it fixed makes
# each fixture differ in exactly one contract violation.
PLAN = None


def plan_clauses():
    global PLAN
    if PLAN is not None:
        return PLAN
    store = Store([])
    clauses, tasks, contexts = load_inventory(store)
    cs, _ = select(clauses, contexts, tasks['T122'])
    PLAN = [c['id'] for c in cs]
    store.close()
    return PLAN


def report(name: str, *, status='DONE', round_=0, toolchain='true', retry='CriticalAndImportant',
           changed=True, tdd=True, ac_extra=False, spec=True, hypotheses=2,
           missing=None, drop_check=None, status_iri=None, unverified_method=None,
           ac_pass=True, drop_method=False, ac_not_run_extra=False):
    out = [HEAD]
    out.append('ar:Report a ar:Report ;')
    out.append('  ar:forTask ar:T122 ;')
    out.append('  ar:hasPlan ar:Plan ;')
    out.append(f'  ar:status {status_iri or "ar:" + status} ;')
    out.append(f'  ar:round "{round_}"^^xsd:integer ;')
    out.append(f'  ar:toolchain {toolchain} ;')
    if retry:
        out.append(f'  ar:retryScope ar:{retry} ;')
    if spec:
        out.append('  ar:specMapping "AC1 -> spec item 1; AC2 -> spec item 2" ;')
    out.append(f'  ar:hypotheses "{hypotheses}"^^xsd:integer ;')
    if missing:
        out.append(f'  ar:missingContext {missing!r} ;')
    out.append('  ar:hasAc ar:AC1, ar:AC2, ar:AC3 ;')
    if changed:
        out.append('  ar:changedFile ar:F1, ar:F2 ;')
    out.append(f'  ar:hasCheck {", ".join("ar:Check" + c for c in plan_clauses() if c != drop_check)} .')
    out.append('')
    out.append('ar:Plan a ar:ImplementationPlan ;')
    for c in plan_clauses():
        out.append(f'  ar:injectsClause ar:{c} ;')
    out[-1] = out[-1].rstrip(' ;') + ' .'
    out.append('')
    for c in plan_clauses():
        out.append(f'ar:Check{c} a ar:Check ; ar:checkClause ar:{c} .')
    out.append('')
    if drop_method:
        out.append('ar:AC1 a ar:AcResult ; ar:acStatus "pass" .')
    elif unverified_method is None:
        out.append('ar:AC1 a ar:AcResult ; ar:acStatus "pass" ; ar:verificationMethod "test_a" .')
    else:
        out.append(f'ar:AC1 a ar:AcResult ; ar:acStatus "pass" ; ar:verificationMethod {unverified_method!r} .')
    out.append('ar:AC2 a ar:AcResult ; ar:acStatus "pass" ; ar:verificationMethod "test_b" .')
    out.append('ar:AC3 a ar:AcResult ; ar:acStatus "pass" ; ar:verificationMethod "test_c" .')

    if ac_not_run_extra:
        out.append('ar:AC1 ar:verificationMethod "not-run" .')
    if not ac_pass:
        out.append('ar:AC3 ar:acStatus "fail" .')
    if ac_extra:
        out.append('ar:AC4 a ar:AcResult ; ar:acStatus "pass" ; ar:verificationMethod "not-run" .')
        out.append('ar:Report ar:hasAc ar:AC4 .')
    out.append(f'ar:F1 a ar:ChangedFile ; ar:changedKind "production" ; ar:hasTestEvidence {str(tdd).lower()} .')
    out.append('ar:F2 a ar:ChangedFile ; ar:changedKind "test" ; ar:hasTestEvidence true .')

    path = ROOT / 'fixtures' / f'{name}.ttl'
    path.parent.mkdir(exist_ok=True)
    path.write_text('\n'.join(out) + '\n')
    return path


def main():
    report('valid', retry=None)
    report('invalid-missing-check', drop_check='I01')
    report('invalid-status', status_iri='ar:MERGE')
    report('invalid-done-unverified-ac', ac_extra=True)
    report('invalid-unverified-no-method', status='UNVERIFIED', unverified_method='not-run')
    report('invalid-done-no-method', drop_method=True)
    report('invalid-done-method-mixed', ac_not_run_extra=True)
    report('invalid-blocked-one-hypothesis', status='BLOCKED', hypotheses=1)
    report('invalid-changed-files-empty', changed=False)
    report('invalid-tdd-missing', tdd=False)
    report('invalid-retry-scope', round_=1, retry='CriticalOnly')
    report('invalid-retry-scope-missing', round_=2, retry=None)
    report('invalid-toolchain-done', toolchain='false')
    report('invalid-spec-mapping', spec=False)
    report('invalid-needs-context-empty', status='NEEDS_CONTEXT', missing=None)
    print('wrote fixtures')


if __name__ == '__main__':
    main()
