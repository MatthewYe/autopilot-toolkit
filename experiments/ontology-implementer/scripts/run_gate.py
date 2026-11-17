#!/usr/bin/env python3
"""Run the implementer report-contract gate over fixtures."""
from __future__ import annotations
import json, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from lib import Store

ROOT = Path(__file__).resolve().parents[1]
EXPECTED = {
    'valid': 0,
    'invalid-missing-check': 1,
    'invalid-status': 1,
    'invalid-done-unverified-ac': 1,
    'invalid-unverified-no-method': 1,
    'invalid-blocked-one-hypothesis': 1,
    'invalid-changed-files-empty': 1,
    'invalid-tdd-missing': 1,
    'invalid-retry-scope': 1,
    'invalid-toolchain-done': 1,
    'invalid-spec-mapping': 1,
    'invalid-needs-context-empty': 1,
}


def main():
    shapes = ROOT / 'shapes' / 'report-shapes.ttl'
    rows = []
    ok = True
    for name, expected in EXPECTED.items():
        store = Store([ROOT / 'ontology' / 'implementer.ttl',
                       ROOT / 'data' / 'tasks.ttl',
                       ROOT / 'data' / 'contexts.ttl',
                       ROOT / 'fixtures' / f'{name}.ttl'])
        result = store.shacl(shapes)
        store.close()
        got = result['exit_code']
        passed = got == expected
        ok = ok and passed
        rows.append({'fixture': name, 'expected': expected, 'exit_code': got,
                     'conforms': result['conforms'], 'passed': passed,
                     'violations': result['violations']})
    out = ROOT / 'results' / 'gate.json'
    out.write_text(json.dumps(rows, indent=2) + '\n')
    lines = []
    for r in rows:
        status = 'PASS' if r['passed'] else 'FAIL'
        lines.append(f"{status} {r['fixture']:34s} expected={r['expected']} exit={r['exit_code']} conforms={r['conforms']}")
        for v in r['violations'][:4]:
            lines.append(f"    {v}")
    text = '\n'.join(lines)
    (ROOT / 'results' / 'gate.txt').write_text(text + '\n')
    print(text)
    print(f"\n{sum(1 for r in rows if r['passed'])}/{len(rows)} fixtures matched")
    return 0 if ok else 1


if __name__ == '__main__':
    raise SystemExit(main())
