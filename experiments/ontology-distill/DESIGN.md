# 把 Distill 改成「本体约束 + 条件注入」：设计与消融实验

状态：实验设计 + 离线验证完成，运行时改造未落地（Phase 1 提案）。
范围：`autopilot-toolkit` 的 `autopilot-distill`（Stage 技能 `grill-with-docs` / `to-spec` / `to-tickets` 保持不修改，ADR 0013）。

## 0. 结论摘要

- **可行的改造点不是重写技能，而是在技能外面加一层“事实驱动的上下文包”**：把三个阶段的规则拆成 45 条带来源锚点的 clause（其中 8 条由 runner 强制，不注入），把 25 张仓库文档卡作为按条件检索的 context，阶段开始时按 `stage + task facts` 选出最小包注入给模型。
- **条件注入的收益主要来自“少注入”而不是“多注入”**：相对“整阶段包”，本体选择把 clause 精度从 0.845 提到 1.000，注入字符从 6110 降到 4263（−30%）；相对“全量注入”降 70%。但 clause 精度是同一作者用同一批事实写 gold 与条件得到的，**不是独立证据**；独立信号只有 stage→ontology 的增量、context 精度、以及关键词检索的失败。
- **约束层必须放在“人确认之前”**：现在 distill 的 checkpoint 只校验证据 JSON 的结构，不校验草稿内容；人确认后再跑门禁会使确认失效。方案是把 SHACL 门禁放在 `testing-seam-confirmed` / `slice-breakdown-approved` 之前，并把草稿哈希写进 checkpoint 证据。
- **门禁只能拦结构不变量**：用 4 份真实 PRD（0002/0003/0004/#121）和 1 组真实 issue 集（#122–#126）回放，18/18 夹具符合预期；5/5 已批准工件通过 blocking 层，0 个 blocking 失败。条件型规则（跨运行时计划、迁移说明、验证命令、CI 触发、文件路径）在历史工件上普遍不满足，只能先做 advisory——**如果直接设成 blocking，会拒绝 3/4 份人已批准的 PRD**。
- **在线模型消融尚未执行**：本环境没有模型 API 凭据，也没有可复现的 runner 预算。本文给出预注册的在线实验方案（5 臂 × 8 任务 × 3 次重复）与判定规则，离线部分只回答“选择与门禁是否按设计工作”，不回答“模型产出是否更好”。

## 1. 现状与缺口

Distill 是四阶段状态机（`crates/distill-cli/workflows/distill.v1.json`）：

| stage | 执行者 | checkpoint | 当前校验 |
|---|---|---|---|
| intake | runner | `explicit-text-captured` | 原文快照（ADR 0010） |
| clarification | skill `grill-with-docs` | `clarification-complete` | `validate_clarification_evidence`（`crates/distill-cli/src/main.rs:888`） |
| prd | skill `to-spec` | `testing-seam-confirmed` | 证据 JSON 结构、feature_slug |
| issues | skill `to-tickets` | `slice-breakdown-approved` | `publication.rs` 的 issue 结构、回执 |

三个缺口：

1. **规则只存在于技能正文里**。`skills/upstream/.../to-spec/SKILL.md` 与 `to-tickets/SKILL.md` 用自然语言列出要求，模型每次全量阅读，无法按任务裁剪，也无法机械检查。
2. **校验只看证据结构，不看草稿内容**。PRD 可以缺 Problem Statement、故事可以不编号、issue 可以没有 acceptance criteria，只要 JSON 字段齐全就能过 checkpoint。
3. **确认与内容不绑定**。人确认 `testing-seam-confirmed` 后，草稿若再被修改，runner 没有哈希可发现，确认覆盖的是一份旧内容。

## 2. 目标与非目标

目标：

- 规则的唯一来源是本体（clause + context card），技能正文只保留流程与证据协议；
- 阶段入口按 `stage + task facts` 条件注入最小上下文，注入量可度量、可审计；
- 草稿在人确认前过机械门禁，门禁失败不允许进入 checkpoint；
- 消融实验能区分“条件选择”与“全量/阶段包/关键词”的差异，且可复现。

非目标：

- 不修改 upstream 三个阶段技能（ADR 0013）；
- 不把语义质量（故事是否是“对的故事”、seam 是否是“最高点”）做成门禁；
- 不在本实验中改 Rust runner（只给出补丁点与 JSON 契约）；
- 不用 LLM 评委替代人做最终质量判定。

## 3. 架构

### 3.1 三层

```
事实层  ontology/distill.ttl      45 clauses, 8 tasks, stage/axis/check kinds
        data/contexts.ttl         25 context cards, path + line range + excerpt
              │  build_artifacts.py（单一来源，带 source anchor）
              ▼
注入层  scripts/lib.py select()   stage + conditions → clause/context 集合
        scripts/select_ablation.py 五种选择臂，渲染 stage pack
              │
              ▼
执行层  shapes/gate-shapes.ttl    blocking（结构不变量）
        shapes/advisory-shapes.ttl advisory（条件规则，先告警）
        scripts/run_gate.py       夹具 + 真实语料回放
```

### 3.2 本体的形状

- **Clause**：`clauseId, axis, stage, text, defaultCheck, sourceAnchor, keyword, when*, checkedBy, enforcedBy`。
  - `defaultCheck` ∈ Runner / Gate / Human / Judgement；只有 Gate 进入 SHACL，Human 与 Judgement 永不自动判定。
  - `enforcedBy Runner` 的 8 条（快照、阶段顺序、yield 字段、用户授权、回执、会话身份、漂移、本地发布）**不注入**——runner 已经强制，注入只是噪声。这是“本体不是把技能抄一遍”的关键：37 条可注入，8 条显式排除。
- **Context card**：仓库文档的稳定切片（path + startLine/endLine + ≤360 字符 excerpt + stage + when*）。卡片是“指针 + 摘要”，不是全文，避免把仓库文档塞进上下文。
- **Task facts**：`changeKind, surface, runtime, ambiguity` 与布尔事实（`multiModule, crossRuntime, multiSlice, domainDocs, adr, newAdr, externalTracker, wideRefactor, userFacing, hasExistingSeam`）。这些是条件注入的唯一输入。

### 3.3 选择语义

```
clauses  = { c | c.stage = stage, c.enforcedBy ≠ Runner,
                 ∀ when* : matches(when*, task facts) }
contexts = { x | stage ∈ x.stages ∨ Cross ∈ x.stages,
                 ∀ when* : matches(when*, task facts) }
```

- 条件之间是 AND；同一条件的多个值是 OR（例：`whenSurface ["Skills","Install"]` 表示 surface 命中任一即可）。
- clause 按阶段严格切分（本体的 45 条里没有 Cross clause，Intake 的 6 条全部 runner-owned）；只有 context card 支持 `Cross`（2 张）。
- **未知事实按 true 处理**（保守注入）：宁可多注入，不可漏掉一条 blocking 规则。代价是 context 过注入，见 §5.3。
- 空条件的 clause 对所有任务生效：可注入的 37 条里 25 条没有 `when` 条件。这是本体的诚实结论——**Distill 的大部分规则本来就不分任务**；条件性主要集中在阶段包与 context 选择，而不是 clause 本身。

### 3.4 注入契约

阶段入口产出 `context_pack`：

```json
{
  "run_id": "...", "stage": "prd", "task_facts": {"crossRuntime": true, "...": "..."},
  "clauses": [{"id": "P10", "text": "...", "check": "Gate"}],
  "contexts": [{"id": "ADR_0044", "path": "docs/adr/0044-...md", "startLine": 1, "endLine": 40, "excerpt": "..."}],
  "dropped": [], "budget": {"chars": 4596, "cap": 6000}
}
```

- 渲染顺序：stage → axis → clauseId；context 按相关度（条件命中数）再按 path 排序。
- 预算（Phase 1 合同，本实验只度量不执行截断）：单阶段 6000 字符（约 1.5k tokens）。超限时**先丢 context excerpt，再丢 Judgement clause 的 text，绝不丢 Gate clause**；仍超限则报错中止，而不是静默截断。这条来自 conditional-context 的教训：cap 一旦 binding，实验比较的就不是选择策略而是截断策略。
- 两种渲染：`full`（clause 全文 + excerpt）与 `hybrid`（id + 一行摘要 + 指针）。实验同时测两种字符成本；运行时默认 `full`，因为 skill 不应再去读文件。

### 3.5 与 distill-cli 的集成（Phase 1 补丁点）

1. **阶段入口**：`crates/distill-cli/src/main.rs` 的 stage dispatch 调 sidecar `distill-context --stage <stage> --facts <json>`，把 `context_pack` 放进 yield。sidecar 缺失时 fail closed（不静默降级为“无注入”）。
2. **提交证据**：`submit-evidence` 在写 checkpoint 前调 `distill-gate --stage <stage> --draft <file>`。Gate 失败返回 violation 列表，不写 checkpoint。
3. **确认哈希**：checkpoint 证据新增 `draft_sha256`；runner 在记录确认时重算哈希并入库。再次提交不同哈希的草稿要求重新确认。补上 §1 的第 3 个缺口。
4. **事实记录**：intake 时写入 `task_facts`，clarification 阶段由人可见（pack 头部展示），避免“注入理由不可见”。
5. **实现选择**：Phase 1 用 Python sidecar（与 `open-ontologies` 同栈，复用本实验脚本），Rust 只做调用与 fail-closed；Phase 3 再考虑把十余条 SHACL 检查改写成 Rust 原生谓词，去掉运行时二进制依赖。

理由：ADR 0041 规定状态机拥有工作流控制，所以 gate 的调用点必须在 runner，而不是技能自觉；但把 RDF/SHACL 引擎塞进 Rust runner 会把实验依赖变成发布依赖，先用 sidecar 隔离。

## 4. Grilling 决策记录

**Q1：为什么不直接把技能正文改写成规则列表？**
改写会违反 ADR 0013，且把“规则”和“流程”绑死。本体只承载规则，技能继续承载流程与证据协议；adapter 组合两者。

**Q2：为什么 gate 不放宽成“警告”了事？**
blocking 层只保留 5 份真实工件全部满足的结构不变量（章节、故事编号、seam、out-of-scope、issue 的 Parent/AC/DAG），所以 0 误拒；advisory 层承载尚未被实践满足的条件规则。两层都保留，才能同时做到“不挡人”和“暴露缺口”。

**Q3：为什么 P03 从“所有故事都必须 As a/I want/so that”降为“必须编号”？**
真实语料里 0002 有 8/9、0003 有 17/18、0004 有 11/14 符合完整句式。把句式当 blocking 会拒绝 3/4 份人已批准 PRD。句式漂移降为 advisory（<80% 告警），编号仍是 blocking——编号是后续 issue 追溯的前提。

**Q4：为什么条件规则不先设成 blocking，等模板修好再放开？**
这正是本实验要避免的“规范先行”错误：门禁如果比实践严，团队会绕过它（改模板措辞、拆小 PR），而不是改行为。先 advisory，积累 2–3 个 run 的分布，再按预注册阈值升级。

**Q5：为什么用 task facts 而不是让模型自己判断该读哪些文档？**
模型自选不可复现、不可审计，也无法做选择消融。facts 是显式、可记录、可在 clarification 被人纠正的输入。代价是 facts 由 agent 赋值，存在判断偏差；用 pack 头部展示 + 人确认来约束。

**Q6：为什么 gate 检查“filePathCount > 0”是 advisory 而不是 gate？**
真实 PRD 全部引用源码路径（0002: 13 处，0004: 25 处）。to-spec 的原意是“不要写文件级实现计划”，但历史工件把“指出受影响的模块/文件”当成澄清手段。语义上二者难分，机械规则只能告警。

**Q7：上下文包会不会让模型忽略技能正文？**
会，所以 pack 只包含“本阶段新增/易漏的约束”，不重复技能正文的流程。Ablation 的 `stage` 臂就是“只给阶段包、不给条件”的对照；如果 ontology 臂不优于它，说明条件选择没有价值，应退回阶段包。

**Q8：为什么不在本 PR 直接改 runner？**
runner 改动牵涉工作流 JSON、checkpoint 证据 schema、双运行时（DSH/Kimi/Codex）兼容与发布工件，属于跨模块协调改造。实验先固定 JSON 契约与门禁语义，Phase 1 再动 Rust；否则实验结论会被实现细节绑架。

## 5. 离线实验与结果

### 5.1 选择消融（24 个 task-stage，5 臂）

| arm | clause recall | clause precision | context recall | context precision | exact set | mean chars (full) | mean chars (hybrid) |
|---|---|---|---|---|---|---|---|
| none | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | 0 | 0 |
| generic | 1.000 | 0.281 | 1.000 | 0.263 | 0.000 | 14235 | 5097 |
| stage | 1.000 | 0.845 | 1.000 | 0.614 | 0.000 | 6110 | 1850 |
| keyword | 0.095 | 0.476 | 0.392 | 0.618 | 0.000 | 1900 | 351 |
| ontology | 1.000 | 1.000 | 1.000 | 0.857 | 0.417 | 4263 | 1427 |

读法：

- `stage` 是“阶段包、忽略条件”，是条件注入的真正对照。ontology 相对它：clause 精度 +0.155，字符 −30%，context 精度 +0.243。
- `keyword` 的 clause recall 只有 0.095：规则的抽象措辞（“seam 是最高点”）与需求原文几乎没有词面重叠，词面检索在本任务上不可用。这条是否定“用 RAG 代替本体”的直接证据。
- **clause precision 1.000 不可当作独立证据**：gold 与 `when*` 条件由同一作者依据同一批事实写成，存在循环。独立可解释的只有 stage→ontology 的增量、context 精度、以及 keyword 的失败。
- context 仍有 14% 过注入（0.857）：粗粒度 `surface` 让 ADR 0044/0045/0008、PRD 0004 范例卡进入并不需要它们的 skill/install 任务。要修只能把卡片拆细或引入更细的 facts，这会让 facts 赋值成本上升——当前选择是接受 14%，换取可解释性。

### 5.2 门禁夹具与真实语料回放

- 18 个夹具全部按预期：blocking 突变（缺 Problem、故事不编号、无 seam、无 out-of-scope、issue 缺 Parent/AC、DAG 环、悬空 blocked-by）全部被对应 shape 拒绝；advisory 六种条件各产生恰好 1 条警告；空图返回 exit 2（conformance undetermined），门禁 fail closed。
- 真实语料：4 份 PRD + 1 组 issue 集，**blocking 0 失败**；advisory 分布：0002/0003 缺运行时计划（P10），0004/#121 缺验证命令（P12），全部 PRD 引用源码路径（P04），0004 故事句式漂移（P03）。

### 5.3 这些数字不能证明什么

- 不能证明模型产出的 PRD/issue 更好；离线实验只测“选择是否按设计工作”和“门禁是否误拒”。
- 8 个任务全部来自同一个仓库，且是事后挑选的（有完整工件可回放），存在选择偏差。
- gold 由单人标注，clause 清单也由同一人从技能/ADR 提炼，存在库存偏差；没有第二标注者，没有 inter-annotator agreement。
- gate 只覆盖结构；一个结构完整但语义空洞的 PRD 仍会通过。语义质量只能靠人。

## 6. 消融实验设计（在线部分，预注册）

完整预注册见 `ABLATION.md`。摘要：

- **问题**：在相同模型与预算下，条件本体注入是否比阶段包/关键词/全量注入产生更高的门禁通过率与更低的注入成本？
- **设计**：5 臂（none / generic / stage / keyword / ontology）× 8 任务 × 3 次重复 = 120 次运行；任务内配对，运行顺序随机化；gate 始终开启（gate 本身由夹具与回放单独验证）。
- **主指标**：最终 issue 集与 PRD 的 blocking gate 通过率（二元）。
- **次指标**：advisory 数、盲评 rubric（0–2/条）、输入 token、墙钟时间、人工修改行数。
- **判定**：H1 ontology 对 stage 非劣（差值 ≥ −10pp）；H2 ontology 注入字符比 stage 少 ≥25%；H3 ontology 优于 keyword；H4 advisory 数低于 stage。任一主指标未达则不上默认，保留 stage pack。
- **为什么不现在跑**：本环境无模型凭据；且 8 任务的配对检验只能发现大效应，预注册里明确写出“若 Phase 2 要检测 10pp 差异，需要 16+ 任务”。

## 7. 分阶段落地

| 阶段 | 内容 | 成功标准 | 回滚 |
|---|---|---|---|
| P0（本 PR） | 本体、选择器、gate 夹具、回放、预注册 | 本文与 `ABLATION.md` 可复现 | 纯实验目录，无运行时影响 |
| P1 | Python sidecar + runner 调用 + checkpoint 哈希，env flag 关闭态 | 现有 distill 测试全绿；开 flag 后 3 次 smoke 通过 | flag 关闭即回旧路径 |
| P2 | 在线消融 120 次 + 盲评 | 预注册判定 | 保持 stage pack 默认 |
| P3 | 按结果决定默认；Rust 原生谓词替换 sidecar | 默认路径无 Python 依赖 | 保留 sidecar 兼容 |

## 8. 风险与未解问题

- **facts 赋值偏差**：谁赋 facts、何时复核、是否允许人改，直接影响注入集合。当前设计是 agent 赋值 + clarification 可见，但未验证。
- **advisory 的归宿**：P10–P13 长期 advisory 会退化成噪声；需要在 P2 后用真实 run 分布决定“升级/删除/改模板”。
- **sidecar 的部署**：Python 运行时在用户机器上不一定存在；Phase 1 需要把打包问题一并解决，否则 fail closed 会让 distill 直接不可用。
- **与 DSH/Kimi/Codex 三运行时的一致性**：pack 是纯文本，理论兼容；但各运行时 skill 变体的证据协议不同，需在 P1 逐一 smoke。
- **语义质量**：gate 永远无法覆盖；若团队把 gate 通过当成“PRD 合格”，会降低评审强度。文档与 UI 需要明确 gate 只是必要条件。

## 9. 复现

```bash
cd experiments/ontology-distill
export OPEN_ONTOLOGIES_BIN=/path/to/open-ontologies   # v2.1.0
python3 scripts/build_artifacts.py     # 重新生成本体/卡片/任务
python3 scripts/select_ablation.py     # results/selection.{json,md,txt}, payloads/
python3 scripts/build_fixtures.py      # fixtures/*.ttl + EXPECTED.json
python3 scripts/run_gate.py            # results/gate.{json,md}, extracted/
```

依赖：Python 3.11+，`open-ontologies` v2.1.0（`--no-connect --data-dir`）。无需网络、无需模型 API。
