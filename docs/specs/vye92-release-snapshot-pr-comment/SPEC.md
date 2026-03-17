# CI/CD：Release Snapshot 队列化发布与 PR 版本评论（vye92）

## 状态

- Status: 部分完成（5/6）
- Created: 2026-03-17
- Last: 2026-03-17

## 背景 / 问题陈述

`catnap` 当前仍使用单体 `CI Pipeline` 在 `push main` 时同步完成主分支校验、release intent 判定与正式发布。这条链路依赖 GitHub `commits/{sha}/pulls` 在发布时反查关联 PR，再读取单个 `type:*` 标签决定是否自动发版。

该方案已经在现网复现漏发：

- latest stable release 仍停留在 `v0.8.1`
- `main` 上合并提交 `ae69817350a3b5aa9924bf2f887ab11a9dd3c497`（PR #70）与 `9d98e8fac9f01cd4ee27ed03e33d786b99d1c7cd`（PR #69）都在 `Release Intent (gate)` 日志中报出 `no associated PR ...; policy: skip auto release`
- 预期版本分别应为 `v0.9.0` 与 `v0.10.0`，但都没有发布

主人已经明确要求**完全放弃**当前 `push main + release-intent + bump_level/tag backfill` 架构，改为与已解决案例一致的 immutable snapshot + split workflows + queued release 做法，并补上 PR 版本评论能力。

## 目标 / 非目标

### Goals

- 把单体 `CI Pipeline` 拆为 `CI PR`、`CI Main`、`Release`，移除 `main` push 中直接做 release-intent / release 的职责。
- 引入 immutable release snapshot，并使用 `refs/notes/release-snapshots` 冻结每个已合并 PR 的发布元数据。
- Release 只消费 snapshot，并支持 backlog drain，解决 burst merge、晚补发旧 commit、`latest` 被旧版本回写等问题。
- 标签模型升级为强制 `type:* + channel:*` 双维度；未来新 PR 必须显式声明 `channel:stable` 或 `channel:rc`。
- 手动补发统一改为 `release.yml workflow_dispatch(commit_sha)`；废弃旧 `bump_level` 手工发版语义与 tag 输入型 `release-backfill.yml`。
- 发布成功后自动向对应 PR 写入版本评论，采用 Issues timeline comment upsert，并暴露明确失败诊断。
- 修复合并后按顺序补发 `ae69817350a3b5aa9924bf2f887ab11a9dd3c497 -> v0.9.0` 与 `9d98e8fac9f01cd4ee27ed03e33d786b99d1c7cd -> v0.10.0`。

### Non-goals

- 不保留旧 `workflow_dispatch ref=main + bump_level=*` 作为长期兼容入口。
- 不继续兼容“只有 `type:*` 也能发版”的长期模式；历史无 `channel:*` 的未发布 PR 只做一次性 stable 回填。
- 不扩展到全仓历史漏发审计；本轮只闭环当前已确认的两个缺失版本。

## 范围（Scope）

### In scope

- `docs/specs/vye92-release-snapshot-pr-comment/SPEC.md` 与 `docs/specs/README.md`
- `.github/workflows/ci-pr.yml`
- `.github/workflows/ci-main.yml`
- `.github/workflows/release.yml`
- `.github/workflows/label-gate.yml`
- `.github/workflows/review-policy.yml`
- `.github/quality-gates.json`
- `.github/scripts/release_snapshot.py`
- `.github/scripts/test-release-snapshot.sh`
- `.github/scripts/verify_manifest_platforms.py`
- `.github/scripts/smoke-test-image.sh`
- `.github/scripts/check_quality_gates_contract.py`
- `.github/scripts/test-quality-gates-contract.sh`
- `.github/scripts/test-inline-metadata-workflows.sh`
- `.github/scripts/metadata_gate.py`
- `README.md`

### Out of scope

- 新的 semver 规则、release assets 矩阵或镜像命名空间改名
- 运行时 API / UI 功能改动
- 全量历史 PR 标签迁移或自动批量补写 `channel:*`

## 需求（Requirements）

### MUST

- workflow 拆分
  - `CI PR` 承接 PR / merge queue required checks，保留 `Path Gate`、`Lint & Checks`、`Backend Tests`、`Release Chain Smoke (PR)` 这些 check name。
  - `CI Main` 只负责 `push main` 校验与 snapshot 固化，不直接发布。
  - `Release` 只接受 `workflow_run(CI Main success)` 或 `workflow_dispatch(commit_sha)`。
- snapshot
  - 使用 git notes `refs/notes/release-snapshots` 保存 immutable JSON snapshot。
  - snapshot 至少包含：`target_sha`、`pr_number`、`pr_title`、`type_label`、`channel_label`、`release_bump`、`release_channel`、`app_effective_version`、`release_tag`、`tags_csv`。
  - snapshot 生成要按 `main` first-parent 顺序补齐缺失祖先，再给目标 commit 分配版本，保证晚补发旧 commit 仍能拿到更早 semver。
  - 写 notes 时必须具备可重试 publish 语义，避免并发 run 产生同版本重复分配。
- 标签模型
  - 新 PR 必须且仅能有一个 `type:*` 与一个 `channel:*`。
  - `channel:*` 当前允许 `channel:stable` 与 `channel:rc`。
  - 对历史已合并但尚未发布、且缺少 `channel:*` 的 PR，snapshot 回填允许默认映射 `channel:stable`。
- release
  - 发布版本、tag、manifest tags、`latest` 资格全部由 snapshot 决定，不再在 release 时重新计算 `bump_level`。
  - `latest` 仅允许由当前仍未被更新 stable snapshot 超车的 stable release 更新。
  - 自动 run 完成后若仍存在 pending snapshot，必须自动 dispatch 下一次 `Release` 直到队列清空。
  - 手动补发 `workflow_dispatch(commit_sha)` 必须拒绝未通过 `CI Main` 的 commit。
- PR 评论
  - 发布成功后以固定 marker upsert 对应 PR 评论，正文至少包含 `release_tag`、`app_effective_version`、`release_channel`、`target_sha` 与 release 链接。
  - job 权限显式声明 `issues: write` 与 `pull-requests: write`。
  - 评论失败时日志必须输出状态码与 `x-accepted-github-permissions` 诊断。
- 文档
  - README 必须切换到 snapshot / queue / `type:* + channel:*` / `workflow_dispatch(commit_sha)` 新口径。
  - 必须明确 `release-backfill.yml` 与旧 `bump_level` 手工发版入口已退役。

## 验收标准（Acceptance Criteria）

- Given 新 PR 缺少 `type:*` 或 `channel:*`，When 触发 `PR Label Gate`，Then required check fail early。
- Given `main` 上连续 squash merge 多个可发布 PR，When `CI Main` 与 `Release` 运行，Then 每个 commit 都会先拥有 immutable snapshot，再按顺序发布，不会因为关联 PR 反查失败而跳过。
- Given 较新的 stable commit 已先发布，When 之后手动补发更老的 stable snapshot，Then 旧版本不会抢回 `latest`。
- Given `Release` 自动 run 因单队列只拿到较新的 `workflow_run` 触发，When notes 中仍有更早 pending snapshot，Then `next-pending` 会先发布较早 commit，再继续 drain backlog。
- Given 发布成功，When 对应 PR 存在，Then PR 上会出现带固定 marker 的版本评论；重复发布同一 tag 时只更新原评论，不重复刷屏。
- Given 目标 commit `ae69817350a3b5aa9924bf2f887ab11a9dd3c497` 与 `9d98e8fac9f01cd4ee27ed03e33d786b99d1c7cd`，When 修复合并后顺序手动 dispatch `release.yml commit_sha=*`，Then 分别产出 `v0.9.0` 与 `v0.10.0`，并对账 Release / GHCR / PR 评论全部成功。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `bash ./.github/scripts/test-release-snapshot.sh`
- `bash ./.github/scripts/test-quality-gates-contract.sh`
- `bash ./.github/scripts/test-inline-metadata-workflows.sh`
- `bash ./.github/scripts/test-live-quality-gates.sh`
- `bash ./.github/scripts/test-prepare-docker-binaries.sh`
- `/bin/bash -n .github/scripts/smoke-test-image.sh`
- `python3 -m py_compile .github/scripts/metadata_gate.py .github/scripts/check_live_quality_gates.py .github/scripts/check_quality_gates_contract.py .github/scripts/release_snapshot.py .github/scripts/verify_manifest_platforms.py`
- `ruby -e 'require "yaml"; %w[.github/workflows/ci-pr.yml .github/workflows/ci-main.yml .github/workflows/release.yml .github/workflows/label-gate.yml .github/workflows/review-policy.yml].each { |path| YAML.load_file(path) }'`

### Quality checks

- 不允许 `main` 上继续存在旧 `release-intent` / `bump_level` 自动发布入口。
- 不允许 branch protection required check 名称漂移。

## 文档更新（Docs to Update）

- `README.md`
- `docs/specs/README.md`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新建规格并冻结 `v0.8.1` / `ae698...` / `9d98...` 现状证据。
- [x] M2: 拆分 `CI PR` / `CI Main` / `Release`，退役旧 monolithic release 路径。
- [x] M3: 引入 immutable snapshot、pending queue 与 latest 保护逻辑。
- [x] M4: 标签门禁升级到 `type:* + channel:*`，并同步 contract / README / tests。
- [x] M5: 发布成功后 upsert PR 版本评论。
- [ ] M6: fast-track 合并修复 PR，顺序补发 `v0.9.0` / `v0.10.0` 并完成线上对账。

## 方案概述（Approach, high-level）

- 直接以已验证方案为蓝本：拆分 workflow，新增 release snapshot 脚本与回归测试，再把 release 阶段迁移为单队列消费 snapshot。
- 保留现有 release asset / GHCR 封装能力，但其版本、tag 与 latest 决策全部改由 snapshot 驱动。
- 用 `workflow_dispatch(commit_sha)` 做补发，先补 `ae698...`，再补 `9d98...`，确保版本递增与 `latest` 指向一致。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：历史缺失版本超过当前识别范围时，后续仍需单独审计，但本轮不扩 scope。
- 风险：历史未发布 PR 缺少 `channel:*` 时需要一次性 stable 映射；该兼容仅限 snapshot 回填，不代表长期放宽规则。
- 开放问题：无。
- 假设：当前缺失版本仅为 PR #70 / #69 对应的 `v0.9.0` / `v0.10.0`。

## 变更记录（Change log）

- 2026-03-17: 新建规格，覆盖旧 `z7myy` 中“不迁移 release 架构”的限制，切换到全量对齐方案。
- 2026-03-17: 已落地 split workflows、immutable release snapshot、queued release、`type:* + channel:*` 门禁与 PR 版本评论；待 fast-track PR 合并后补发 `v0.9.0` / `v0.10.0`。
