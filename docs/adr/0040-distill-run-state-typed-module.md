# Distill run state is a typed module

## Context

distill-cli's run state (`state.json`) was manipulated through string-key
`serde_json::Value` access scattered across the binary: the report projection
read dozens of `state["..."]` keys, `start` and `supersede` each carried a
duplicated 60-line initial-state literal, transition guards were inlined in
handlers, and three inline copies of the commit protocol had already diverged
on quota accounting. String keys give no compile-time guarantee that reads and
writes match the on-disk schema, and inlined guards can drift between handlers.

## Decision

1. The `state` module owns a typed `RunState` schema — serde structs and
   enums for the run lifecycle, stage state, purge cleanup state, publication
   status, and boundary state whose serialized values match the existing
   bytes exactly. The load path is `read Value → migrate_state(Value) →
   deserialize RunState`; the write path is `RunState → to_value →
   to_vec_pretty`. The quota preflight and every state write go through the
   same serializer, so preflight byte counts always match the bytes that land
   on disk. Unknown top-level keys survive read-modify-write via a
   `#[serde(flatten)]` catch-all; nested structs stay permissive. The
   heterogeneous arrays (`completion_evidence[]`, `publications.issues[]`)
   are modeled as untagged enums whose variants keep the two on-disk shapes
   distinct. The `implementation_started` dead field is not modeled and
   round-trips through the catch-all.
2. Every legal transition is a method on `RunState` with its guards
   internalized: `complete_stage`, `defer_stage`, `record_boundary`,
   `release_session`, `mark_stage_needs_reconciliation`, `abort`,
   `takeover`, `begin_purge`/`complete_purge`, and `mark_superseded`.
   Methods own their revision bump and return a structured `StateError`
   carrying expected/actual, whose `Display` renders the exact legacy CLI
   error text. Handlers are thin adapters: parse args → load → call one
   transition method → commit. New runs — both `start` and the supersede
   successor — are built by one shared `RunState::new` constructor (with
   `predecessor_run_id` set for the successor) instead of a separate
   `RunState::successor`, eliminating the duplicated initial-state
   literals.
3. `transition::commit` is the single owner of the commit protocol
   (preflight quota → append events → write files → write state), extended
   with `commit_audit_only` for audit-only transitions (state + audit event,
   exempt from the preflight those transitions never had). Purge keeps its
   two-phase durable-pending recovery path and supersede keeps its cross-run
   two-write + rollback orchestration in the handler, by design.
4. The typed schema lives in the distill-cli binary crate as its `state`
   module; `active_run_for_session` moved there from the `workflow` module,
   which is pure workflow-definition navigation again.
5. Behavior preservation is judged by the unmodified integration tests that
   drive the real binary.

## Alternatives considered

### Typestate (encode the run/stage state in the Rust type system)

Rejected: serde round-trip ergonomics. Every persisted snapshot must
deserialize regardless of lifecycle phase, so a typestate encoding fights the
deserializer at exactly the seam where state is loaded; guards that belong on
data would have to be re-expressed as type-level invariants serde cannot
represent.

### Unify the on-disk shapes (one shape for completion evidence and publications)

Rejected: it crosses the line of this work — a refactoring that preserves
behavior and bytes. Unifying the shapes changes what old binaries wrote and
requires a state migration, which the forward-only migration policy
(ADR-0032) reserves for schema versions.

### Extract a library crate for the state module

Rejected: single consumer. distill-cli is the only reader and writer of run
state; a library crate would add workspace machinery without a second
consumer (unlike the ADR-0009 shared crate, which consolidated logic that
several binaries and scripts already duplicated).

### `#[serde(deny_unknown_fields)]`

Rejected: inconsistent with the permissive policy. Unknown keys must survive
read-modify-write cycles so a newer binary can extend state without breaking
older ones; the top-level flatten catch-all preserves them, and nested
structs deliberately stay permissive.

## Consequences

- String-key run-state access is eliminated from the report projection and
  the migrated handlers; schema drift becomes a compile error instead of a
  runtime surprise.
- Each guard exists exactly once; illegal transitions produce structured
  errors whose messages stay byte-identical to the pre-typed CLI.
- The commit protocol's ordering (events → files → state) and quota
  preflight are uniform; abort/takeover's legacy state-before-event order was
  normalized, and the shifted mid-commit partial-failure window is documented
  in `transition.rs`.
- Fields still `Value`-typed inside the schema (`clarification`,
  `requirement_snapshot`, `storage`, `report` payloads) are a deliberate
  scope cut; deepening them is future work.
