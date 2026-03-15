# 发布链路全量重构与防漏发对齐（#qp3ht）

## 状态

- Status: 部分完成（3/4）
- Created: 2026-03-15
- Last: 2026-03-15

## 背景 / 问题陈述

- 当前发布链路把 PR CI、`push main` 检查与正式发版混在同一个 workflow 里，`release-intent` 只看“当前 head 对应的单个 PR”。
- 这条链路对 `commits/{sha}/pulls` 解析结果过于依赖；当 merge commit subject 不带 `(#123)` 尾缀时，像 PR `#70`、`#69` 这样的连续合并会被错误判成 `should_release=false`。
- GitHub Actions 的 concurrency 即使对 main 设为“不取消”，仍可能让旧 pending main run 被新 pending 顶掉；如果版本决策仍是“单 commit -> 单次 bump 现场计算”，就会漏掉未发布提交。
- 现有质量门仍把 `Review Policy Gate` 放在 required checks 里，这和目标态的 GitHub-native review enforcement 不一致，也让仓库内合同与远端 branch rules 出现职责重叠。

## 目标 / 非目标

### Goals

- 将工作流拓扑拆成 `PR CI`、`Main CI`、`Release Pipeline`、`Release Reconcile` 四段，明确“PR 可取消、main/release 不取消”。
- 将发布判定从“当前 head 的单个 PR”改为“最新 stable tag 之后、目标 SHA 之前的未发布候选按 first-parent 顺序逐个对账”。
- 收紧 PR 标签合同为 `type:* + channel:*`，新增 `channel:rc`，并让 stable / rc 都走同一套 candidate planning/publish 代码路径。
- 将 repo-local quality gates 调整为 GitHub-native review 目标态；`Review Policy Gate` 保留为过渡期诊断，但退出 repo-local required checks。
- 合并后可通过手动 reconcile 顺序补发本次漏掉的 `#70` / `#69` stable releases。

### Non-goals

- 不改变业务运行时 API / UI 行为。
- 不在本计划内直接修改 GitHub 远端 ruleset / branch protection 配置。
- 不把连续 merged PR 批量压缩成单个版本；继续保持“一次可发布 merged commit 对应一个 release”的语义。

## 范围（Scope）

### In scope

- `.github/workflows/ci-pr.yml`、`.github/workflows/ci-main.yml`、`.github/workflows/release.yml`、`.github/workflows/release-backfill.yml`。
- `.github/scripts/release_plan.py`、`.github/scripts/release-intent.sh` 与新的 release planning regression tests。
- `PR Label Gate` 的 `type:* + channel:*` 双标签强约束，以及相关 fixture / contract tests。
- `.github/quality-gates.json`、live/contract checks 与 README / spec 文档同步。
- `channel:rc` 远端标签库存对齐与合并后 reconcile runbook。

### Out of scope

- Docker image naming、release asset target matrix、smoke test 基本策略。
- 新的 release registry / artifact storage。
- PR merge strategy（merge / squash / rebase）本身的仓库规则修改。

## 需求（Requirements）

### MUST

- PR 必须且只能有一个 `type:*` 与一个 `channel:*` 标签。
- 自动发布必须从“最新 reachable stable tag”向前扫描 first-parent main 历史，并按顺序为每个可发布提交计算版本与 tag。
- `channel:stable` 产出 `vX.Y.Z` 并只让最后一个 stable candidate 标记 `latest`；`channel:rc` 产出 `vX.Y.Z-rc.<sha7>` 且不得更新 `latest`。
- auto release 与 manual reconcile/backfill 必须复用同一 candidate planner 与同一 publish pipeline。
- direct push、无法可靠映射 PR、或标签非法时，自动发布必须继续保守 skip；manual reconcile 必须显式给出原因而不是静默通过。
- repo-local required checks 不得再依赖 `Review Policy Gate`。

### SHOULD

- 对历史上缺少 `channel:*` 的已合并 PR，manual reconcile 与 auto planner 默认按 `channel:stable` 兼容，以便补齐旧版本缺口。
- 发布 workflow 的矩阵执行顺序应保持顺序化，避免 `latest` 在旧版本先发布时被抢先移动。

## 功能与行为规格（Functional/Behavior Spec）

### Workflow topology

- `PR CI`
  - 触发：`pull_request`、`merge_group`
  - concurrency：`cancel-in-progress: true`
  - required jobs：`Path Gate`、`Lint & Checks`、`Backend Tests`、`Release Chain Smoke (PR)`
- `Main CI`
  - 触发：`push` to `main`
  - concurrency：`cancel-in-progress: false`
  - 只负责质量验证，不再直接产出 release
- `Release Pipeline`
  - 触发：成功完成的 `Main CI`
  - 先跑 planner，再按 matrix 顺序发布每个 candidate
- `Release Reconcile`
  - 手动输入 `target_ref`
  - 复用 planner，按缺口顺序补发

### Candidate planning

- planner 以最新 stable tag 为 base version；若没有 stable tag，则 fallback 到 `Cargo.toml` 的 semver。
- 每个 first-parent commit 按顺序执行：`commit -> PR -> labels -> release candidate`。
- PR 解析顺序固定为：
  1. `commits/{sha}/pulls`
  2. `closed pulls` 中 `merge_commit_sha == sha`
  3. commit subject fallback（`Merge pull request #123` / `... (#123)`）
- `type:docs|type:skip` 只记录到 skipped list，不生成 release candidate。
- `channel:rc` 使用下一次 stable bump 对应的 semver core，再派生 prerelease tag / app version。

### Quality gates transition

- `.github/quality-gates.json` 的目标态切到 `github-native` review enforcement。
- 过渡期允许 live branch rules 仍多一个 `Review Policy Gate` required check，但它只能作为 tolerated extra drift，不再是 repo-local required truth。
- `Review Policy` workflow 继续存在，作为远端 ruleset 尚未调整完成前的诊断/兼容层。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `release_plan.py inspect-commit` | CLI | internal | New | inline in this spec | CI | release-intent / tests | 单 commit 发布意图分类 |
| `release_plan.py plan` | CLI | internal | New | inline in this spec | CI | release pipeline / reconcile | 逐提交生成 release matrix |
| `Release Reconcile` | GitHub Actions workflow | internal | Replace | inline in this spec | maintainers | ops | 以 `target_ref` 触发补发 |

## 验收标准（Acceptance Criteria）

- Given 连续两个 merged commits 分别对应 `type:minor + channel:stable`
  When 只跑最后一个 `Main CI` / `Release Pipeline`
  Then planner 仍必须按顺序产出 `v0.9.0`、`v0.10.0` 两个 stable releases，而不是只发最后一个。

- Given 某个 merged commit 的 PR 标签为 `type:patch + channel:rc`
  When planner 生成 release candidate
  Then 产出 `vX.Y.Z-rc.<sha7>` prerelease tag，且 `publish_latest=false`。

- Given merge commit subject 不包含 `(#123)`
  When `commits/{sha}/pulls` 返回空集合但 closed pull 的 `merge_commit_sha` 可匹配
  Then 自动发布仍能解析到正确 PR 并继续发版。

- Given PR 缺少 `channel:*`
  When label gate 在 PR 阶段执行
  Then CI 失败并明确提示必须补齐且只能保留一个 `channel:*` 标签。

- Given live branch rules 仍额外要求 `Review Policy Gate`
  When 仓库内 live quality gate checker 运行
  Then 它可以把该 check 视为 tolerated extra，但 repo-local required checks 仍只认 GitHub-native review 目标态。

## 实现前置条件（Definition of Ready / Preconditions）

- `channel:stable` 现有 label 保持可用；新增 `channel:rc` label 允许在本计划 PR 阶段创建。
- 当前仓库没有 open PR，可直接收紧 label contract 而无需迁移窗口。
- 漏发版本按“一提交一 release”补齐语义已经冻结。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `bash ./.github/scripts/test-release-intent.sh`
- `python3 ./.github/scripts/test-release-plan.py`
- `bash ./.github/scripts/test-quality-gates-contract.sh`
- `bash ./.github/scripts/test-live-quality-gates.sh`
- `ruby -e 'require "yaml"; (Dir[".github/workflows/*.yml"] + Dir[".github/actions/**/*.yml"]).sort.each { |p| YAML.load_file(p) }'`

### Quality checks

- `python3 -m py_compile .github/scripts/release_plan.py .github/scripts/test-release-plan.py .github/scripts/check_live_quality_gates.py .github/scripts/check_quality_gates_contract.py .github/scripts/metadata_gate.py`

## 文档更新（Docs to Update）

- `README.md`：发布拓扑、`type:* + channel:*`、stable/rc、manual reconcile 说明。
- `docs/specs/README.md`：登记该 spec。
- 历史 specs 保留为记录，但本 spec 作为新的 release governance truth source。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新建总规格，冻结“逐提交补发、不做批量发版”的目标语义。
- [x] M2: 拆分 `PR CI` / `Main CI` / `Release Pipeline` / `Release Reconcile` 并接上 shared publish pipeline。
- [x] M3: 重构 release planner、label gate 与质量门合同，并补齐回归测试。
- [ ] M4: 创建 PR、补齐远端 `channel:rc` label、收敛 checks / review-loop，并记录合并后补发 `#70` / `#69` 的 runbook。

## 方案概述（Approach, high-level）

- 用 `release_plan.py` 统一承接“单 commit 分类”和“按 target SHA 规划 release matrix”两种语义，避免 shell 脚本继续堆叠分支逻辑。
- 自动发布从 `workflow_run(Main CI)` 出发，确保 main 上的验证与发布决策解耦，同时让 release job 不再被 PR concurrency 语义污染。
- 手动 reconcile 只要求提供 `target_ref`，其余版本规划沿用同一条代码路径；历史缺失 `channel:*` 的 merged PR 默认按 stable 兼容。
- review policy 先完成 repo-local 合同切换，再允许 live rules 通过 tolerated extra 过渡，直到仓库设置手动跟上。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：`Release Reconcile` 目前仍以“最新 stable tag 之后的缺口”为主路径；若未来需要把“已存在 stable tag 但 release / GHCR 缺失”的场景也统一纳入 planner，需要额外扩展 planner 输入。
- 风险：`Review Policy Gate` 在远端 ruleset 切换完成前仍会继续出现在 live required checks 中，因此 transition allowance 必须保持同步，不能被误删。
- 假设：`ncipollo/release-action@v1` 的 `prerelease` / `makeLatest` 输入在当前 pinned major 版本下保持兼容。

## 合并后操作（Post-merge runbook）

### GitHub settings

- 在仓库 `main` ruleset / branch protection 中移除 `Review Policy Gate` required status check。
- 保留 native pull-request approvals = `1`，并维持 `dismiss_stale_reviews_on_push=false`、`require_code_owner_review=false`、`require_last_push_approval=false`。

### Missing release reconcile

- 创建远端 label：`channel:rc`
- 合并本 PR 后，在 Actions 手动触发 `Release Reconcile`：
  - `target_ref=main`
  - `legacy_missing_channel=stable`
- 预期 planner 按顺序补出：
  - PR `#70` -> `v0.9.0`
  - PR `#69` -> `v0.10.0`
- 验收证据：GitHub Releases、release assets、GHCR `v0.9.0` / `v0.10.0` / `latest` 对账一致。

## 参考（References）

- `docs/specs/8btwa-release-intent-label-gating/SPEC.md`
- `docs/specs/z7myy-release-intent-squash-fallback-backfill/SPEC.md`
- `docs/specs/tbpgt-release-automation-alignment/SPEC.md`
- `docs/specs/pgnnw-release-ghcr-chain-fix/SPEC.md`
