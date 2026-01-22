# CI/CD：GitHub Actions 构建提速（#0006）

## 状态

- Status: 已完成
- Created: 2026-01-21
- Last: 2026-01-22

## 背景 / 问题陈述

- 本仓库当前仅有一个 GitHub Actions workflow：`.github/workflows/ci.yml`（同时覆盖 `pull_request` / `push main` / `workflow_dispatch`）。
- workflows 关键 job：
  - `Lint & Checks`：Bun（lint/typecheck/storybook/test-storybook）+ Rust fmt/clippy/check
  - `Backend Tests`：web build（用于 UI embed）+ cargo test
  - `Release Chain Smoke (PR)`：release build + smoke test + `docker/build-push-action`（no push）
  - `Release (tag/assets/image)`：Release assets（4 targets）+ multi-arch image push（Buildx + QEMU）
- baseline 观测（PR run，核心瓶颈在 Docker multi-arch 构建）：
  - Run: `IvanLi-CN/catnap` → Actions run `21198139759`（`pull_request`，2026-01-21）
  - `Release Chain Smoke (PR)` job：约 44 分钟（`05:15:45Z` → `05:59:28Z`）
    - 其中 `Docker build (no push)` step：约 41 分钟（`05:17:53Z` → `05:59:21Z`）
  - 同一 run 内：
    - `Backend Tests` job：约 37 秒
    - `Lint & Checks` job：约 83 秒（其中 “Front-end lint/build” 约 51 秒）
- 静态勘察（可疑的重复/低效点）：
  - `Release Chain Smoke (PR)` 在 workflow 内已执行 `cargo build --release`，但 `Dockerfile` 仍会再次执行：web build + `cargo build --release`（Docker 多阶段构建），存在重复构建与缓存失效风险。
  - PR 的 docker build 默认 `platforms: linux/amd64,linux/arm64`，在 amd64 runner 上构建 arm64 很可能落入 QEMU（极慢），并成为本仓库当前 PR 的主要耗时来源。
  - 当前仅对 cargo dir 做了 `actions/cache@v4`；Bun 依赖与 Playwright 浏览器下载没有明确缓存策略，且前端任务在多个 job 重复执行（`lint` / `unit-tests` / `pr-release-check` / `release`）。

## 目标 / 非目标

### Goals

- 显著缩短 PR 的 wall-clock，优先把 `Release Chain Smoke (PR)` 的 `Docker build (no push)` 从 ~41 分钟降到“可接受的 PR 反馈时延”。
- 在不牺牲“可解释性与可回滚性”的前提下减少重复构建：避免同一条 CI 里多次 build web / build release binary / 再 Docker 里重复编译。
- 让提速结果可验证：CI 日志/summary 能直接看出 cache hit/miss、gating 决策与其收益。

### Non-goals

- 不修改 Catnap 的业务行为、对外 API 契约或数据模型。
- 不切换 CI 平台（仍使用 GitHub Actions）。
- 不默认引入付费/自建 runner（如需用到，必须主人显式批准并写入本计划）。
- 不包含 “修复 `push main` Release job 失败” 的工作（另见 Plan #0007）。

## 用户与场景

- 维护者：希望 PR 快速拿到可靠的 CI 结论；频繁 push/rebase 时不被慢 CI 拖累。
- 贡献者：后端变更不应被“QEMU multi-arch Docker 构建”强制拖慢；涉及 Docker/Release 相关变更才触发相应重型校验。
- 发布流程：Release 仍需稳定可重跑；cache 异常时要可诊断且不应“假成功”。

## 需求（Requirements）

### MUST

- 方案分档：至少给出 3 档（保守/均衡/激进），每档明确：
  - 会改动哪些 job/step（以及是否引入额外 runner 类型）；
  - 预期收益来源（减少重复构建 / 缓存提升 / gating 跳过重型步骤 / 避免 QEMU 等）；
  - 风险与回滚策略（如何一键回到“全量跑”）。
- PR 的 `Release Chain Smoke (PR)` 必须不再默认进行“QEMU multi-arch 的全量 docker build”：
  - 冻结：PR 阶段跳过 `linux/arm64` docker build，仅构建 `linux/amd64`。
- 缓存策略必须明确且可测试：
  - Cargo cache：继续以 `Cargo.lock` 作为 key 的核心输入；
  - Bun cache：以 `web/bun.lock*`（或等价 lockfile）作为 key 的核心输入；
  - Playwright browsers cache（若仍保留 `bunx playwright install`）：以 Playwright 版本（来自 lockfile）作为 key 的核心输入；
  - Docker build cache：使用 Buildx cache backend `type=gha`，并明确“cache 写入失败”为 best-effort（不应导致 build 失败）。
- 保持现有质量门槛不被“偷跑”：
  - Rust fmt/clippy/check 与 cargo test 的失败必须继续 fail CI；
  - 对 Storybook/test-storybook 是否允许 gating，必须由主人明确决策并写入契约与验收。
- 时间目标（验收基线，冷 cache）：
  - 冻结：PR workflow 总耗时 ≤ 10 分钟。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| CI gating rules (by paths/labels) | File format | internal | New | ./contracts/file-formats.md | maintainer | CI/contributors | 规定“哪些变更触发哪些重型 job” |
| `.github/scripts/ci-path-gate.sh` (planned) | CLI | internal | New | ./contracts/cli.md | maintainer | CI | 输出 `*_changed` gate 信号 |
| Docker build cache semantics | File format | internal | Modify | ./contracts/file-formats.md | maintainer | CI | backend=`type=gha`；cache best-effort，不吞真实 build 错误 |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/cli.md](./contracts/cli.md)
- [contracts/file-formats.md](./contracts/file-formats.md)

## 约束与风险（Constraints & Risks）

- QEMU 在 arm64 构建/编译场景下可能极慢：本计划已冻结 PR 阶段跳过 arm64；Release 路径的 arm64 优化不在本计划范围内。
- Docker build cache backend 可能偶发不稳定（尤其是 cache export）：需要 best-effort 语义与可诊断输出。
- 引入 gating 会改变“覆盖面”：需要明确哪些改动必须强制跑重型 job，避免漏检。
- 当前 `push main` 的 Release job 已观测到失败（Actions run `21201355431`，失败在 “Determine release version/tag”）：修复已另立计划（Plan #0007）。

## 验收标准（Acceptance Criteria）

- Given 一个 PR 指向 `main` 且变更未命中 `Dockerfile`、`.github/**`、`Cargo.toml`/`Cargo.lock`、`src/**`、`web/**`
  When CI 运行
  Then `Release Chain Smoke (PR)` 必须跳过，且 CI 总耗时 ≤ 10 分钟

- Given 一个 PR 指向 `main` 且变更命中上述任一“重型触发路径”
  When CI 运行
  Then `Release Chain Smoke (PR)` 必须运行，且该 workflow 总耗时 ≤ 10 分钟

- Given Docker build cache backend 暂时不可用（cache export 失败）
  When CI 运行 docker build
  Then job 不应因“仅 cache 写入失败”而失败；同时日志必须输出明确 warning（但真实 build 失败仍必须 fail）

## 实现前置条件（Definition of Ready / Preconditions）

- 冻结已完成：
  - PR 跳过 `linux/arm64` docker build
  - 前端重型检查允许 gating（触发路径见契约）
  - Docker build cache backend=`type=gha`（best-effort）
  - PR workflow 总耗时 ≤ 10 分钟（冷 cache）
- 契约文档已定稿（见 `./contracts/*`）。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests：继续运行 `cargo test --locked --all-features`（失败必须 fail CI）。
- Smoke test：继续运行 `.github/scripts/smoke-test.sh`（失败必须 fail CI）。

### UI / Storybook (if applicable)

- 默认保留 `bun run build-storybook` + `bun run test:storybook` 的质量门槛；是否允许 gating 由主人决策后冻结。

### Quality checks

- 继续运行 `cargo fmt --check`、`cargo clippy -D warnings`、`cargo check`；不引入新的 lint/typecheck 工具。

## 文档更新（Docs to Update）

- `README.md`: 补充/更新 CI 运行说明（特别是：PR gating 口径、何时会跑重型 job、以及 cache 的 best-effort 语义）。
- `docs/plan/README.md`: 新增本计划索引行（已完成）。

## 实现里程碑（Milestones）

- [x] M1: 采集并固化 PR baseline（至少 1 个 run），把关键 step timing 写入 job summary
- [x] M2: 引入 Bun/Playwright 缓存策略并验证命中（不改变既有检查口径）
- [x] M3: 落地 PR gating + PR docker build 仅 `linux/amd64`（并补齐 buildx cache best-effort）
- [x] M4: 文档同步（README/plan notes），确保贡献者可理解触发规则与排障入口

## 方案概述（Approach, high-level）

- 方案 A（保守）：PR docker build 固定仅 `linux/amd64`（no push）+ 启用 Buildx cache（`type=gha`, best-effort）。
- 方案 B（均衡）：在方案 A 基础上，引入 PR gating：后端/文档 PR 默认跳过 `Release Chain Smoke (PR)` 与前端重型检查；触发规则冻结在契约中。
- 方案 C（激进）：进一步减少重复构建（例如对 Dockerfile 的构建路径做等价调整，减少 Docker 内二次编译）；不改变最终镜像行为。

## 开放问题（Open Questions）

- None

## 假设（Assumptions）

- None

## 变更记录（Change log）

- 2026-01-21: 冻结口径：PR 跳过 arm64；允许前端 gating；Docker cache backend=`type=gha`；目标 PR ≤ 10 分钟。
- 2026-01-22: 实现：新增 `ci-path-gate.sh` 并在 CI 中落地 PR gating；补齐 Bun/Playwright cache；Docker build cache 切到 `type=gha` 并提供 best-effort fallback；同步 README 与 Index 状态。
- 2026-01-22: 补齐：为 PR 的 `Release Chain Smoke (PR)` job 增加 timings summary；baseline 参考 Actions run `21235535755`（workflow ~3m，PR smoke job ~2m31s）。

## 参考（References）

- 对标：`IvanLi-CN/dockrev` PR #11（docs-only plan 模式）：`https://github.com/IvanLi-CN/dockrev/pull/11`
- baseline PR run：Actions run `21198139759`：`https://github.com/IvanLi-CN/catnap/actions/runs/21198139759`
- Release job failure fix plan（out of scope）：Plan #0007
