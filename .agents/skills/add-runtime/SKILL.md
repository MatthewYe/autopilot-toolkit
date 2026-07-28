---
name: add-runtime
description: "Checklist-driven guide for adding a new agent runtime to autopilot-toolkit. Use when extending autopilot to a new runtime (e.g. Claude Code, Cursor, Windsurf, Copilot). Reference for the predictable 12-touchpoint pattern discovered from adapting Reasonix, Codex, Kimi, and OpenCode."
---

# Add Runtime

Add a new agent runtime to the autopilot-toolkit. This skill is reference-only — every step is a concrete file edit with explicit patterns to copy from existing runtimes.

## Model

Every runtime adaptation touches the same 12 touchpoints, always in this order. Each touchpoint has a **pattern** — the file and section to copy from an existing runtime — and a **check** — the command to verify that step.

### 0. Decide variant format

Before editing anything, decide which variant file format the runtime uses for coupled skills:

| Runtime pattern | Format | Example | Agent file |
|----------------|--------|---------|------------|
| SKILL.md variants | Per-runtime `SKILL.md` in each skill dir | reasonix, kimi | None (inlined) |
| Agent config | Runtime-native agent file | codex → `agent.toml`, opencode → `agent.md` | `agent.toml` / `agent.md` |

For runtimes that dispatch tasks to subagents, prefer the agent config pattern — the orchestrator references them by name and the runtime loads them natively.

**If the runtime uses agent configs:** create `<runtime>/agent.md` (or `.toml`) for implementer and reviewer.
**If not:** create `<runtime>/SKILL.md` for every coupled skill.

---

### 1. Skill variant files

Create runtime-specific files under each coupled skill directory. Start from an existing variant file and adapt the dispatch model.

```
skills/autopilot/
├── autopilot-implementer/<runtime>/agent.md    # or SKILL.md
├── autopilot-reviewer/<runtime>/agent.md       # or SKILL.md
├── autopilot-orchestrator/<runtime>/SKILL.md   # always SKILL.md
├── autopilot-distill/<runtime>/SKILL.md        # always SKILL.md
└── audit-autopilot/<runtime>/SKILL.md          # always SKILL.md
```

**Key adaptation points per skill:**

- **implementer**: Dispatch preamble (how the orchestrator invokes it), tool names, skill-loading instructions. Copy from the same skill's nearest runtime variant, then adapt the dispatch model section.
- **reviewer**: Same as implementer — preamble, tools, skill loading.
- **orchestrator**: The largest file. Copy from reasonix/opencode variant. Adapt: dispatch model (task/spawn/run_skill), tool names, file operations, status update mechanism, issue tracker interaction. Preserve all sections: Issue source, Prerequisites, AFK Contract, Recovery Decision Model, Spec detection, Scan mode, Phase 1 loop, Phase 2 meta-review, VERDICT branches.
- **distill**: Session identity discovery. The `--runtime` flag and session-id resolution are runtime-specific. Write a concise `<runtime>`-native session discovery paragraph, then reuse the shared distill CLI contract.
- **audit-autopilot**: Session discovery + trace format.

**Pattern — orchestrator dispatch model by runtime:**

| Runtime | Dispatch tool | Preamble format |
|---------|--------------|----------------|
| reasonix | `run_skill(name: "...")` | Describe task inline |
| codex | `spawn agent <name> with task:` | Describe task inline |
| kimi | Agent tool | "Read SKILL.md, then follow" |
| opencode | `task(subagent_type: "...")` | `skill(name: "...")` loading block |

**Check:** `rust-script validation/run.rs` — opencode variants appear with [PASS].

---

### 2. skill-index: RUNTIME_VARIANTS

**Pattern:** `crates/skill-index/src/lib.rs` line ~65

Add the runtime name to the sorted array:
```rust
const RUNTIME_VARIANTS: &[&str] = &["codex", "kimi", "opencode", "reasonix"];
```

Also update the `discover_skills_in_this_repo` test to assert the new variant is found:
```rust
assert!(orch.variants.contains(&"<runtime>".to_string()));
```

**Check:** `cargo test -p skill-index`

---

### 3. validation: SkillVariant enum

**Pattern:** `crates/validation/src/lib.rs` lines ~38-48

Add variant to the enum. If the runtime allows opencode-specific frontmatter fields (`compatibility`, `mode`, `permission`, `hidden`, `arguments`), model it after `Codex`/`Kimi`/`Opencode`. Otherwise, model after `Reasonix`/`Agnostic`.

```rust
pub enum SkillVariant {
    Reasonix,
    Codex,
    Kimi,
    Opencode,
    /// <new> runtime ...
    <NewRuntime>,
    Agnostic,
}
```

Then in `validate_skill_with_variant`, add `<NewRuntime>` to the opencode-field exclusion list (line ~260) if appropriate.

**Check:** `cargo test -p validation`

---

### 4. validation-runner: variant mapping + status check

**Pattern A — variant mapping:** `crates/validation-runner/src/lib.rs` line ~159

```rust
Some("<runtime>") => SkillVariant::<NewRuntime>,
```

**Pattern B — status check:** Same file, after `check_codex_status` (~line 440)

Copy `check_opencode_status` and adapt:
- Change directory checks (`opencode` → `<runtime>`)
- Change agent file check (`agent.md` → appropriate extension)
- Call it in the report block after the opencode section

**Pattern C — global check exclusion:** Same file, lines ~261, ~275

Add `<runtime>` to the opencode-field exclusion filter:
```rust
v != Some("<runtime>")
```

**Check:** `cargo test -p validation-runner && rust-script validation/run.rs`

---

### 5. deploy: main entry + env vars

**Pattern:** `deploy.rs` lines ~56-60

Add env var overrides:
```rust
let <runtime>_skills_dir = env::var("<RUNTIME>_SKILLS_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|_| PathBuf::from(&home).join(".<runtime>/skills"));
let <runtime>_agents_dir = env::var("<RUNTIME>_AGENTS_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|_| PathBuf::from(&home).join(".<runtime>/agents"));
```

Pass them to `dev_all` and `dev_clean` calls (lines ~88-117).

**Check:** `cargo build -p deploy`

---

### 6. deploy/dev: dev_all + dev_clean

**Pattern A — dev_all:** `crates/deploy/src/dev.rs`

Add `<runtime>_skills_dir` and `<runtime>_agents_dir` parameters. Inside the coupled skill block:
- If the runtime uses agent files: deploy them (pattern after `codex_agent` or `opencode_agent` blocks)
- If the runtime uses flat `.md` in its skills dir: create symlinks there
- `remove_project_symlink` for cleanup

Also add symlinks for agnostic and upstream skills if the runtime needs them (pattern after opencode's flat `.md` approach).

**Pattern B — dev_clean:** Same file.

Add `<runtime>` dirs to the cleanup loops (both skills and agents dirs).

**Check:** `cargo build -p deploy`

---

### 7. deploy/lib: stage_coupled_skill

**Pattern:** `crates/deploy/src/lib.rs`

Three locations need the new runtime added to variant lists:
- Line ~192: `fallback_variant` array
- Line ~226: skip list for non-variant files
- Line ~254: variant copy loop
- The router text (line ~212): add runtime name to the dispatch instruction
- The doc comment (line ~181-183): add runtime to the layout diagram

**Check:** `cargo test -p deploy`

---

### 8. bootstrap.sh

**Pattern:** `bootstrap.sh`

Add `--target <runtime>` support:

1. **usage()**: Add the target description and behavior notes
2. **Target validation**: Add `<runtime>` to the allowed targets list
3. **Path resolution**: Add `elif` branch in the TARGET_SKILLS_DIR block
4. **Agent deployment**: If the runtime uses agent files, add a deployment block modeled after the `codex` or `opencode` sections
5. **Skill symlinks/commands**: If the runtime needs flat `.md` files or command wrappers, add a section modeled after the `opencode` block

**Key decision — skill discovery strategy:**

Runtime-native discovery patterns:
- **SSOT-native** (codex, reasonix): Runtime reads `~/.agents/skills/` directly. Only agent files need deployment to runtime-specific dirs. Legacy cleanup removes old per-skill symlinks.
- **File-symlink** (opencode): Runtime only reads its own dir. Create `.md` symlinks from `~/.agents/skills/<name>/SKILL.md` to `~/.<runtime>/skills/<name>.md`.
- **Command-wrapper** (opencode): When flat files aren't enough, generate wrapper `.md` with frontmatter extracted from the canonical SKILL.md.

Pick the right strategy for the new runtime.

**Check:** `bash bootstrap.sh --target <runtime>` (after dev or install)

---

### 9. install.sh.in

**Pattern:** `templates/install.sh.in` lines ~35-46

Add detection and bootstrap for the new runtime:
```bash
if [[ -d "${HOME}/.<runtime>" ]]; then
    echo "Bootstrapping <Runtime>..."
    <RUNTIME>_SKILLS_DIR="${<RUNTIME>_SKILLS_DIR:-${HOME}/.<runtime>/skills}" \
    <RUNTIME>_AGENTS_DIR="${<RUNTIME>_AGENTS_DIR:-${HOME}/.<runtime>/agents}" \
    "${BOOTSTRAP_SCRIPT}" --target <runtime>
fi
```

**Check:** `bash dist/install.sh --tarball dist/autopilot-toolkit.tar.gz`

---

### 10. uninstall.sh

**Pattern:** `templates/uninstall.sh`

Add runtime variable definitions and `cleanup_symlinks` calls, plus agent-file cleanup if the runtime uses them.

**Check:** `bash ~/.agents/skills/.autopilot/uninstall.sh`

---

### 11. distill-cli: SUPPORTED_RUNTIMES

**Pattern A — args.rs:** `crates/distill-cli/src/args.rs` line ~5
```rust
pub(crate) const SUPPORTED_RUNTIMES: [&str; N] = ["codex", "kimi", "opencode", "reasonix"];
```
Add the new runtime and increment the array size.

**Pattern B — main.rs:** `crates/distill-cli/src/main.rs` line ~93
Add to the help text: `<codex|kimi|opencode|reasonix>`.

**Pattern C — tests/runtime.rs:** Update the supported runtime test loop and the error message assertion.

**Check:** `cargo test -p distill-cli`

---

### 12. CI + Tests + Docs

**CI** (`.github/workflows/ci.yml`):
- Add runtime-specific test job if applicable (pattern after `test_kimi_distill`)
- No changes needed for fmt/clippy unless new test files are added

**Tests:**
- `tests/test_install.rs`: Add variant to mock project creation, add env var overrides
- `tests/test_build.rs`: Add variant to mock project for tarball test
- `tests/test_github_verify.rs`: Add assertions for the new runtime
- `tests/test_toolkit_setup.rs`: Add `--target <runtime>` handling + TestContext fields

**Docs:**
- `AGENTS.md` line 3: Update runtime list
- `AGENTS.md` architecture diagram: Add runtime directory
- `AGENTS.md` install model: Mention runtime
- `README.md` line 3: Update runtime list

**Check:** `cargo test --all && rust-script validation/run.rs`

---

## Failure modes

- **Forgetting the distill-cli update.** Most distinct from the rest — easy to miss. The symptom: `ERROR: --runtime must be one of: ...` despite all other steps being correct.
- **Wrong skill discovery strategy.** Copying the pattern from the wrong runtime. Codex reads SSOT natively; opencode needs flat files. Study the new runtime's skill-loading mechanism before picking a strategy.
- **Variant list drift.** `stage_coupled_skill` has three separate hardcoded variant lists. Missing one causes staging or dev to silently skip the new variant.
- **bootstrap.sh idempotency.** The legacy cleanup loop removes symlinks for ALL targets. If the new runtime needs symlinks, add them AFTER the cleanup — the cleanup runs first.
- **Test mock project exclusion.** `test_install.rs` and `test_build.rs` have hardcoded variant lists in mock fixtures. Missing the new runtime causes no test failure — the variant is simply never tested.
