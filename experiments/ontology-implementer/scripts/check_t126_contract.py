#!/usr/bin/env python3
"""Structural hidden check for T126: the entry-level resolution field is gone."""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

repo = Path(sys.argv[1]).resolve()
lib = repo / "crates/skill-index/src/lib.rs"
text = lib.read_text()

m = re.search(r"pub struct ExpectedSetEntry\s*\{(.*?)\n\}", text, re.S)
if not m:
    result = {"pass": False, "reason": "ExpectedSetEntry not found"}
else:
    body = m.group(1)
    has_field = bool(re.search(r"\bpub\s+resolution\s*:", body))
    refs = []
    for p in list((repo / "crates").rglob("*.rs")) + list((repo / "tests").rglob("*.rs")):
        for lineno, line in enumerate(p.read_text(errors="ignore").splitlines(), 1):
            if re.search(r"\b(?:entry|expected_set_entry)\s*\.\s*resolution\b", line):
                refs.append(f"{p.relative_to(repo)}:{lineno}: {line.strip()}")
    result = {
        "pass": not has_field and not refs,
        "entry_resolution_field": has_field,
        "entry_resolution_refs": refs,
    }
print(json.dumps(result, indent=2))
sys.exit(0 if result["pass"] else 1)
