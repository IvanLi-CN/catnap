# CI/CD：发版自动化（GHCR + GitHub Release + Release Assets）对标与补齐（#0003）

## 状态

- Status: 待实现
- Created: 2026-01-20
- Last: 2026-01-21

## 背景 / 问题陈述

当前仓库已存在 GitHub Actions 工作流（`.github/workflows/ci.yml`），会在 `main` 上：

- 计算 `APP_EFFECTIVE_VERSION`（`.github/scripts/compute-version.sh`）
- 自动创建 tag + GitHub Release
- 构建并推送 Docker 镜像到 GHCR（`Dockerfile`）

但现状仍缺少“唯一且可重复”的发版口径与验收标准，导致：

- 当前 workflow 存在 `release: published` 触发声明，但我们不使用该路径（历史遗留；并且当前 jobs 也未覆盖该触发的发布路径）
- `push main` 会无条件进入“自动发版链路”，而 `compute-version.sh` 会在 tag 已存在时自动递增 patch：当合并内容仅为文档/设计类变更时，仍会产生新的版本迭代（无实际产物变化）
- GitHub Release 未附带可复用的 release assets（仅有 release notes）
- Docker 镜像仅构建 `linux/amd64`（需扩展到 `linux/arm64`）
- PR 阶段没有对“发布链路”的最小构建/运行校验（容易合并后才发现发布失败）

本计划参考 `IvanLi-CN/dockrev` 的计划 PR #5，冻结 Catnap 的发版产物形态、触发策略、契约与验收标准，以便后续进入实现阶段一次性补齐。

## 已确认决策（主人已拍板）

- GitHub Release 必须附带 release assets：linux/amd64+arm64（gnu+musl）。
- Web UI 静态资源：embed 到二进制（不依赖运行时文件系统目录）。
- GHCR：单镜像 `ghcr.io/<owner>/catnap`（`push main` 更新 `latest`）。
- 依赖版本 pinning：按“major 级别”为主（具体版本见 `contracts/file-formats.md` 的 toolchain pinning）。
- 手动发版：使用 GitHub Actions 的 `workflow_dispatch`（手动触发 workflow），不使用 `release: published`。
- 解决“docs-only 也发版”的方案选择：采用 **方案 C**（以 PR 标签作为“发布意图（release intent）”的唯一信号；CI 在 PR 阶段强制要求具备有效标签；发布 jobs 在 `push main` 时仅对满足“发布意图”的合并提交生效）。
- 发布意图标签集合（互斥且必须 1 个）：
  - `type:docs`：文档/设计类变更；不得自动发版
  - `type:skip`：不论变更内容为何，显式跳过自动发版（仅 lint/tests；需要时走 `workflow_dispatch`）
  - `type:patch`：允许自动发版；CI 自动做 patch bump 并发布
  - `type:minor`：允许自动发版；CI 自动做 minor bump 并发布
  - `type:major`：允许自动发版；CI 自动做 major bump 并发布
- 无 PR / direct push 策略：`push main` 无法关联 PR 时，默认跳过自动发版（仅 `workflow_dispatch` 可手动发布）。
- base version 选择（用于 bump 起点）：选取仓库现存 `v<semver>` tags 的语义版本最大值；若无任何 `v<semver>` tags，则 fallback `Cargo.toml` 的 `version`（见 `contracts/file-formats.md`）。

## 目标 / 非目标

### Goals

- 定义 Catnap 的**唯一**发版流程（Canonical CI/CD flow），覆盖：
  - tag 与 GitHub Release 策略（幂等）
  - GHCR 镜像命名与 tag 策略
  - GitHub Release assets 的命名与内容约定
  - PR 阶段对发布链路的构建与 smoke test 校验（不 push）
- 避免“无产物变化”的版本迭代：当仅发生文档/设计类变更时，不应触发自动发版（tag/release/GHCR）。
- 保持版本口径稳定：以 `Cargo.toml` 与 `compute-version.sh` 为准，产出 `APP_EFFECTIVE_VERSION=<semver>`。
- 让发布产物可观测：镜像与运行中服务能明确暴露版本信息（OCI labels / env / HTTP API 对齐）。

### Non-goals

- 不在本计划内改动 Catnap 的业务语义与对外 API 设计（仅为满足发版可观测/探活的必要调整除外）。
- 不新增其他分发渠道（例如 Homebrew / APT / crates.io），除非主人另行指定。
- 不扩展到非 Linux 平台（macOS / Windows），除非主人另行指定。

## 用户与场景

- 维护者：合并到 `main` 后，自动得到一个新版本（tag + GitHub Release），并能从 GHCR 拉取对应版本镜像。
- 部署者：希望以固定版本号部署（例如 `v0.1.0`），并可选择是否跟随 `latest`。
- CI：在 PR 阶段就能验证“发版相关构建”不会在合并后失败。

## 范围（Scope）

### In scope

- CI 触发/权限/产物口径对齐：`pull_request` / `push main` / `workflow_dispatch`
- Release assets（tar.gz + checksum）构建与上传（linux/amd64+arm64，gnu+musl）
- Docker build（多架构/标签/缓存/labels）口径对齐与 PR 构建校验
- smoke test：对构建产物做最小运行验证（`/api/health` + `/`）

### Out of scope

- 运行时部署方案的全面设计（K8s/Compose/系统服务化等），除“如何拉取已发布镜像”这类最小文档补齐外。

## 需求（Requirements）

### MUST

- CI/CD 必须能稳定地产出版本号（`APP_EFFECTIVE_VERSION`）并形成一致的 tag（`v<semver>`）。
- 在 `main` 分支上，自动发布流程必须满足：
  - 通过 lint/tests 后才允许进入发布步骤
  - tag/release 创建具备幂等性（重复运行不应破坏仓库状态；行为必须可预测）
  - GHCR 镜像 tags 至少包含 `v<APP_EFFECTIVE_VERSION>`，且默认分支具备 `latest`
- 自动发版（tag/release/GHCR）必须仅在合并 PR 的“发布意图标签”允许发版时触发（`type:major|minor|patch`）；`type:docs|skip` 不得产生新版本（定义见 `contracts/file-formats.md` 的 Trigger contract）。
- GitHub Release 必须附带 release assets（命名/内容/校验和符合契约；目标矩阵：linux/amd64+arm64，gnu+musl）。
- 手动发布必须可用：通过 `workflow_dispatch` 能触发一次“完整发布链路”（tag/release/assets/GHCR），以便在自动发布失败时人工兜底。
  - ref=`main` 时必须显式选择 `bump_level`（`major|minor|patch`）
- PR 阶段必须执行“发布链路构建校验”（不 push），至少包括：
  - Docker build（与 `Dockerfile` 对齐）
  - smoke test（启动→探活→访问 UI）
- 版本可观测：
  - Docker 镜像必须写入版本元数据（OCI labels + runtime env）
  - 运行中服务 `/api/health` 返回的 `version` 必须与 `APP_EFFECTIVE_VERSION` 一致
- Web UI 静态资源必须 embed 到二进制（不依赖 `STATIC_DIR` 文件系统目录）。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `.github/scripts/compute-version.sh` | CLI | internal | Modify | [./contracts/cli.md](./contracts/cli.md) | maintainer | CI | 产出 `APP_EFFECTIVE_VERSION` |
| `.github/scripts/smoke-test.sh` (planned) | CLI | internal | New | [./contracts/cli.md](./contracts/cli.md) | maintainer | CI | 最小运行验证 |
| `.github/scripts/label-gate.sh` (planned) | CLI | internal | New | [./contracts/cli.md](./contracts/cli.md) | maintainer | CI | PR 标签互斥校验与信号导出 |
| `.github/scripts/release-intent.sh` (planned) | CLI | internal | New | [./contracts/cli.md](./contracts/cli.md) | maintainer | CI | `push main` 提交→发布意图映射 |
| Git tag & GitHub Release naming | File format | external | Modify | [./contracts/file-formats.md](./contracts/file-formats.md) | maintainer | users/deployers | `v<semver>` |
| GHCR image naming & tagging | File format | external | Modify | [./contracts/file-formats.md](./contracts/file-formats.md) | maintainer | deployers | `ghcr.io/<owner>/catnap:<tag>` |
| GitHub Release assets naming | File format | external | New | [./contracts/file-formats.md](./contracts/file-formats.md) | maintainer | deployers | tar.gz + sha256 |
| `GET /api/health` | HTTP API | external | Modify | [./contracts/http-apis.md](./contracts/http-apis.md) | backend | CI/users | 探活 + version |
| Static UI routes | HTTP API | external | Modify | [./contracts/http-apis.md](./contracts/http-apis.md) | backend | browsers | `GET /` 与 `/assets/*` |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/cli.md](./contracts/cli.md)
- [contracts/file-formats.md](./contracts/file-formats.md)
- [contracts/http-apis.md](./contracts/http-apis.md)

## 约束与风险（Constraints & Risks）

- `.github/workflows/ci.yml` 当前存在 `release: published` 触发声明：实现阶段将移除，避免出现“触发了但不发布”的误导。
- “发布意图标签”的 enforce 需要足够清晰（互斥规则、缺失/冲突时的错误提示、以及无 PR 时的策略）；否则会在实现阶段反复调整，引入不稳定性。
- Docker build 目前仅 `linux/amd64`：实现阶段需要扩展到 `linux/arm64`。
- Release assets 的 target 矩阵已冻结为 4 targets（amd64/arm64 × gnu/musl），会显著增加 CI 复杂度（交叉编译策略需要可重复且可排障）。
- 依赖版本 pinning 已冻结为 major 级别：实现阶段需要把 CI、Dockerfile、以及发布链路依赖统一收敛到同一组 pin（见 `contracts/file-formats.md`）。
- 当前服务端静态资源来自 `STATIC_DIR`（文件系统目录），与“embed 到二进制”的目标不一致：实现阶段需要改造静态资源提供方式并更新 smoke test。

## 验收标准（Acceptance Criteria）

- Given 合并到 `main` 的 PR 标签为 `type:docs` 或 `type:skip`
  When CI 在 `main` 上运行
  Then 不得创建新的 git tag / GitHub Release，且不得推送新的 `v<semver>` 镜像 tag（可以仅执行 lint/tests）

- Given 当前 base version 为 `vX.Y.Z`（按 `contracts/file-formats.md` 的 base 规则选择）
  And 合并到 `main` 的 PR 标签为 `type:minor`
  When CI 进入发布阶段并计算 `APP_EFFECTIVE_VERSION`
  Then 目标 tag 必须为 `vX.(Y+1).0`（若已存在则继续递增 patch 直到未占用版本）

- Given 合并一个提交到 `main`
  When CI 通过 lint/tests 并进入发布阶段
  Then 若目标 tag `v<APP_EFFECTIVE_VERSION>` 不存在，则自动创建并推送 tag，且创建对应 GitHub Release（包含自动生成的 release notes）

- Given 合并一个提交到 `main`
  When 发布阶段执行镜像构建与推送
  Then GHCR 上存在 `ghcr.io/<owner>/catnap:v<APP_EFFECTIVE_VERSION>`，且默认分支存在 `latest`

- Given 合并一个提交到 `main`
  When 发布阶段创建 GitHub Release
  Then 该 Release 附带 release assets（命名/内容/校验和符合契约；linux/amd64+arm64，gnu+musl）

- Given 有一个 PR 指向 `main`
  When CI 在 PR 上运行
  Then 必须先校验 PR 标签满足“意图标签互斥且必须 1 个”的契约；并执行发布链路的构建校验（不 push），且 smoke test 通过

- Given CI smoke test 启动服务端进程并等待就绪
  When 请求 `GET /api/health`
  Then 返回 `200`，且 JSON 字段 `status=ok`，并包含 `version`

- Given CI smoke test 启动服务端进程并等待就绪
  When 请求 `GET /`
  Then 返回 `200` 且 Content-Type 为 `text/html`（或等价），并包含可识别的 HTML 结构（例如 `<!doctype html>`）

- Given 维护者在 GitHub Actions 手动触发 `workflow_dispatch`（可选选择 ref=main 或某个 tag）
  When CI 运行手动发布路径
  Then ref=`main` 时必须显式选择 `bump_level` 并发布新版本；ref=`refs/tags/v<semver>` 时必须重跑该版本且不更新 `latest`

- Given 发布流程被重复触发（例如重跑 workflow）
  When tag 或 Release 已存在
  Then 行为明确且可预测：要么安全跳过并继续后续步骤，要么失败并给出明确原因（不得留下半成品状态）

## 实现前置条件（Definition of Ready / Preconditions）

（在 Status 变为 `待实现` 或切到 `/prompts:impl` 前必须满足；不满足则保持 `待设计`。）

- 已冻结“发布意图标签契约”：允许/要求的标签集合与互斥规则、以及版本 bump 规则（major/minor/patch），并明确无 PR（直接 push）时的处理策略（见 `contracts/file-formats.md`）。
- 手动发版口径已冻结：通过 `workflow_dispatch` 实现（支持 ref=main 或 tag；仅 main 更新 `latest`）
- 接口契约已定稿（`./contracts/*.md`）

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- 保持现有门槛不退化：
  - `cargo fmt --check`
  - `cargo clippy -D warnings`
  - `cargo test --all-features`
  - `bun run lint`（若 `web/` 存在）
- 新增发布相关校验（PR 阶段）：
  - Docker build（不 push）
  - smoke test（不依赖外部服务；只验证本地启动与 HTTP 可用性）

### Quality checks

- Workflow permissions 最小化：
  - 默认 `contents: read`
  - 发布步骤才允许 `contents: write` / `packages: write`

## 文档更新（Docs to Update）

- `README.md`（如仓库采用）：新增/补齐 Releases / Images / Versioning / Smoke test 说明
- `docs/`（如已有对应文档）：补齐“如何拉取镜像 / 如何使用 release assets”的最小指南

## 实现里程碑（Milestones）

- [ ] M1: Workflow 触发与发布门槛对齐：PR 标签 gate（互斥且必须 1 个）；`push main` 仅对 `type:major|minor|patch` 自动发版；`workflow_dispatch` 手动发版；移除 `release: published`
- [ ] M2: Release assets：4 targets（amd64/arm64 × gnu/musl）构建、打包（tar.gz + sha256）并幂等上传
- [ ] M3: GHCR 多架构发布：`linux/amd64,linux/arm64` 单镜像 `ghcr.io/<owner>/catnap`，含 `v<semver>` 与（main-only）`latest`，并写入 OCI labels
- [ ] M4: smoke test：新增 `.github/scripts/smoke-test.sh` 并接入 PR/发布流程（`/api/health` + `/`）
- [ ] M5: UI embed：将 web 静态资源 embed 到二进制，并保证 `/` 与 `/assets/*` 正常工作
- [ ] M6: 文档同步：补齐 `README.md` 的 Releases / Images / Versioning / Manual publish（workflow_dispatch）说明

## 开放问题（需要主人回答）

None.

## 假设（Assumptions，待主人确认）

- 默认以 PR 标签作为发布意图的唯一信号；`type:docs` 与 `type:skip` 均视为 `should_release=false`（仅 lint/tests）。

## 变更记录（Change log）

- 2026-01-20: 创建计划 #0003，并冻结 release assets 矩阵、UI embed、单镜像与 major 级 pinning。
- 2026-01-20: 冻结触发策略：自动发版 `push main`；手动发版使用 `workflow_dispatch`；移除 `release: published`。
- 2026-01-21: 补充“仅在发布相关变更时自动发版”的需求与验收；计划状态回退为 `待设计`，等待主人冻结触发门槛与方案选择。
- 2026-01-21: 方案切换为 C：以 PR 标签作为发布意图信号；CI 在 PR 阶段强制标签；发布链路以标签 gate。
- 2026-01-21: 冻结标签集合：`type:docs|skip|patch|minor|major`（互斥且必须 1 个）；冻结版本策略：按标签由 CI 自动 bump 并发布。
- 2026-01-21: 冻结无 PR 策略：`push main` 无法关联 PR 时，默认跳过自动发版（仅 `workflow_dispatch` 可手动发布）。

## 参考（References）

- `dockrev` 参考计划（PR #5）：`https://github.com/IvanLi-CN/dockrev/pull/5`
