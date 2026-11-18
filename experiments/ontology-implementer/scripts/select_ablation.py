#!/usr/bin/env python3
"""Compare none / generic / keyword / ontology selection for implementer tickets."""
from __future__ import annotations
import json, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib import PREFIX, Store, load_inventory, select, metrics
from render_plan import render

ROOT = Path(__file__).resolve().parents[1]

KEYWORDS = {
    'I01': ['test', 'tdd'], 'I02': ['behavior', 'behaviour'], 'I03': ['mock'],
    'I04': ['red', 'green', 'refactor'], 'I05': ['acceptance', 'criterion', 'criteria'],
    'I06': ['issue', 'brief'], 'I07': ['glossary', 'context.md'], 'I08': ['adr', 'spec', 'prd'],
    'I09': ['seam'], 'I10': ['cross-issue', 'suggestions'], 'I11': ['retry', 'critical'],
    'I12': ['retry', 'important'], 'I13': ['retry', 'passed'], 'I14': ['retry', 'feature'],
    'I15': ['refactoring'], 'I16': ['diagnos', 'hypothes'], 'I17': ['full suite', 'test suite'],
    'I18': ['coverage'], 'I19': ['scope creep'], 'I20': ['changed_files', 'changed files'],
    'I21': ['implementer_report', 'report'], 'I22': ['done', 'evidence'],
    'I23': ['toolchain'], 'I24': ['unverified'], 'I25': ['blocked'],
    'I26': ['needs_context', 'needs context'], 'I27': ['scope'], 'I28': ['guess'],
    'I29': ['type-check', 'typecheck', 'clippy', 'lint'], 'I30': ['sibling'],
    'I31': ['rollback', 'restore', 'data-loss'], 'I32': ['api-contract', 'caller', 'contract'],
    'I33': ['behavior-preservation', 'behaviour preservation', 'preserve'],
    'I34': ['dependency', 'dependencies'], 'I35': ['sandbox'], 'I36': ['cache'],
    'I37': ['spec', 'deliberate'],
}


def text_of(task: dict, issue_body: str) -> str:
    return ' '.join([task['title'], issue_body, ' '.join(task['changeKinds']),
                     ' '.join(task['risks']), task['runtime'], str(task['specLinked']),
                     str(task['multiModule'])]).lower()


def generic_contexts(contexts, task):
    """All contexts available at this ticket's base.

    A generic arm must not count another ticket's version of the same source
    file: those excerpts are not in this ticket's prepared repo.  Keep global
    contexts (no forTask) and contexts explicitly scoped to this task.
    """
    out = []
    for c in contexts.values():
        tasks = c.get('conditions', {}).get('forTask') or []
        if tasks and task['id'] not in tasks and task['iri'] not in tasks:
            continue
        out.append(c)
    return sorted(out, key=lambda c: c['id'])


def keyword_select(clauses, contexts, task, text):
    cs = [c for c in clauses.values() if any(k.lower() in text for k in KEYWORDS.get(c['id'], []))]
    xs = [c for c in contexts.values()
          if any(k.lower() in text for k in [c['title'].lower(), c['path'].lower()])
          or (c['id'].startswith('CTX_ISSUE_') and str(task['id'][1:]) in c['id'])]
    return sorted(cs, key=lambda c: c['id']), sorted(xs, key=lambda c: c['id'])


def chars(clauses, contexts, mode):
    if mode == 'full':
        return sum(len(c['text']) for c in clauses) + sum(c['chars'] for c in contexts)
    if mode == 'ids':
        return sum(len(c['id']) + 1 for c in clauses) + sum(len(c['id']) + 1 for c in contexts)
    if mode == 'compact':
        return sum(len(c['id']) + 1 for c in clauses) + sum(len(c['title']) + 1 for c in contexts)
    if mode == 'hybrid':
        return (sum(len(c['text']) + 1 for c in clauses)
                + sum(len(c['path']) + len(str(c.get('start', 1))) + 8 for c in contexts))
    raise ValueError(mode)


def main():
    import json as _json
    store = Store([])
    clauses, tasks, contexts = load_inventory(store)
    store.close()
    corpus = {n: _json.loads((ROOT / 'corpus' / f'issue-{n}.json').read_text())['body']
              for n in [122, 123, 124, 125, 126]}

    results = []
    for tid, task in tasks.items():
        body = corpus[int(tid[1:])]
        text = text_of(task, body)
        arms = {}
        arms['none'] = ([], [])
        arms['generic'] = (sorted(clauses.values(), key=lambda c: c['id']),
                           generic_contexts(contexts, task))
        arms['keyword'] = keyword_select(clauses, contexts, task, text)
        arms['ontology'] = select(clauses, contexts, task)

        for name, (cs, xs) in arms.items():
            cids, xids = [c['id'] for c in cs], [c['id'] for c in xs]
            cr, cp, ch, cn, cg = metrics(cids, task['goldClauses'])
            xr, xp, xh, xn, xg = metrics(xids, task['goldContexts'])
            results.append({
                'task': tid, 'arm': name,
                'clause_recall': round(cr, 4), 'clause_precision': round(cp, 4),
                'clause_hit': ch, 'clause_selected': cn, 'clause_gold': cg,
                'context_recall': round(xr, 4), 'context_precision': round(xp, 4),
                'context_hit': xh, 'context_selected': xn, 'context_gold': xg,
                'exact': cids == sorted(task['goldClauses']) and xids == sorted(task['goldContexts']),
                'chars_full': chars(cs, xs, 'full'),
                'chars_ids': chars(cs, xs, 'ids'),
                'chars_compact': chars(cs, xs, 'compact'),
                'chars_hybrid': chars(cs, xs, 'hybrid'),
                'chars_rendered': len(render(task, cs, xs)),
                'selected_clauses': sorted(cids), 'selected_contexts': sorted(xids),
            })

    out = ROOT / 'results' / 'selection.json'
    out.parent.mkdir(exist_ok=True)
    out.write_text(json.dumps(results, indent=2) + '\n')

    lines = []
    for tid in tasks:
        lines.append(tid)
        for arm in ['none', 'generic', 'keyword', 'ontology']:
            r = next(x for x in results if x['task'] == tid and x['arm'] == arm)
            lines.append(
                f"  {arm:9s} clause R/P {r['clause_recall']:.2f}/{r['clause_precision']:.2f} "
                f"({r['clause_hit']}/{r['clause_selected']} sel, gold {r['clause_gold']})  "
                f"ctx R/P {r['context_recall']:.2f}/{r['context_precision']:.2f} "
                f"({r['context_hit']}/{r['context_selected']} sel, gold {r['context_gold']})  "
                f"exact={r['exact']} chars full/ids/compact/hybrid/rendered="
                f"{r['chars_full']}/{r['chars_ids']}/{r['chars_compact']}/{r['chars_hybrid']}/{r['chars_rendered']}")
    report = '\n'.join(lines)
    (ROOT / 'results' / 'selection.txt').write_text(report + '\n')
    print(report)


if __name__ == '__main__':
    main()
