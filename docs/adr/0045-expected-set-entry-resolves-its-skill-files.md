# ADR 0045: An Expected-set Entry Resolves Its Skill Files

> Builds on ADR 0044 (single owner for the Expected set) and ADR 0036 (single
> discoverable router). Sharpens the term recorded as **Skill file** in
> `CONTEXT.md`.

## Context

ADR 0044 made `skill-index` the single owner of Expected-set enumeration, but the
entry it returns only resolved a *directory* (`source_dir`, `resolution`). Which
files that directory owes the toolkit — the root `SKILL.md`, one per runtime
variant, and the two codex variants that ship `agent.toml` instead — stayed
per-consumer folklore. `validation-runner` rebuilt the layout (`source_dir.join
("SKILL.md")`, `variant.join("SKILL.md")`) and privately owned the codex
exemption; `check_codex_status` re-walked `skills/autopilot/*/codex` and de-duped
against the caller's skill list. A resolved entry whose `SKILL.md` was absent
vanished from validation's report while the gate still said PASS — ticket #118
patched that consumer, not the interface.

## Decision

An Expected-set entry resolves the **skill files** it owns, and those files carry
the failure vocabulary. Each skill file records its variant, its resolved path,
its kind (`Skill` for a `SKILL.md`, `AgentDefinition` for an `agent.toml`), and
its resolution (`Resolved`, or `Missing { reason }`). The entry-level resolution
status retires: an entry is a failed entry when any file it owns is missing, and an
entry that malformed-provenance cannot ground reports one missing file anchored at
its source root. A variant that ships an `agent.toml` appears as a resolved
`AgentDefinition`; a variant directory with neither file is a missing `Skill`.

Strictness stays at the consumer, exactly as ADR 0044 recorded it: `dev` warns and
skips failed entries, `pack` fails, validation FAILs. What changed is where the
facts come from — consumers read the resolved file list instead of rebuilding the
source layout. Layout knowledge for the *source tree* (which files an entry owns)
lives in the enumerator; layout policy for the *installed tree* (router, instruction
renaming, runtime directories, fallback priority) stays in `deploy` per ADR 0036.

`validation-runner` stops shadowing the entry with a flattened `Skill` record:
validation targets are built as one list that carries identity and result together,
so report grouping can no longer misalign skills and results. `check_codex_status`
becomes a pure projection of the file list and drops its placeholder-directory INFO
line, which was unreachable through the shipped validation pipeline (such a
directory already fails validation).

## Alternatives considered

### A. Keep directory-level resolution and let consumers derive files

Rejected: it is the status quo that produced #118. Two consumers derived the codex
exemption differently, and the interface could not distinguish "resolved directory"
from "resolved directory with the files the toolkit needs".

### B. A second, file-level status alongside the entry-level one

Rejected: two failure vocabularies for one fact invite drift — the same failure
would be reported at whichever level a consumer happened to read.

### C. Keep the placeholder-directory INFO line in `check_codex_status`

Rejected: unreachable in the shipped pipeline (validation FAILs such a directory),
and it existed only because the function re-derived facts the enumerator now owns.

## Consequences

- Deliberate behavior changes: `check_codex_status` no longer emits the
  placeholder-directory INFO line; entry-level `resolution` and its reason strings
  leave the `ExpectedSetEntry` interface; an `agent.toml` variant is reported as a
  resolved agent definition instead of being parsed as frontmatter; and packaging
  now fails on a skill whose directory exists but whose root `SKILL.md` is absent
  (a state the old directory-level check could not see).
- Everything else keeps its behavior: `dev` policy, `pack` strictness, tarball
  layout, validation verdicts for present, missing-directory, missing-`SKILL.md`,
  and malformed-`skillPath` entries.
- Adding a runtime variant, or a variant that ships a non-`SKILL.md` artifact,
  becomes one enumerator change instead of a change in every consumer.
- `validation-runner`'s skill/result parallel arrays retire; the report iterates one
  list.
