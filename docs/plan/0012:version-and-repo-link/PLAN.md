# 版本号显示 + 仓库地址 + 更新提示（#0012）

## 状态

- Status: 已完成
- Created: 2026-02-14
- Last: 2026-02-14
- PR: #34

## 目标 / 非目标

### Goals

- UI 显示当前运行版本号（后端权威）与仓库地址。
- 新版本提示分两条路径：
  - 部署更新提示：后端更新导致前端 bundle 变化时，UI 在 60s 内提示；点击后直接刷新页面并确保加载到新前端。
  - 上游更新提示：检测 GitHub latest release 与当前版本的差异；如有更新可用，提供提示与跳转链接。
- 离线/受限网络下（GitHub 不可达或限流），应用可用且不崩溃，提示可解释。

### Non-goals

- 不引入复杂的自动升级/自更新机制（仅提示与跳转）。
- 不依赖前端自行访问 GitHub（由后端检查并做缓存）。

## 范围（Scope）

### In scope

- 后端新增：
  - `GET /api/meta`：返回 `effectiveVersion`、`webDistBuildId`、`repoUrl`。
  - `GET /api/update`：服务端检查并缓存 GitHub latest release，返回 `updateAvailable` 与 release 信息。
  - `GET /api/bootstrap` 增加 `app` 字段（包含上述 meta）。
- 前端新增：
  - 侧栏底部展示：`v{effectiveVersion}` + Repo 链接。
  - 顶栏提示：
    - 部署更新提示（基于 `/api/meta` 轮询）；点击触发“安全刷新”（带 cache-buster 参数）。
    - 上游 release 提示（基于 `/api/update`）。
  - 设置页新增 About 区块：版本/构建 id/Repo、复制 Repo、手动检查更新。

### Out of scope

- 不改变现有 release/CI 版本计算策略。

## 验收标准（Acceptance Criteria）

- AC1（Deploy update）Given 后端部署导致 `webDistBuildId` 或 `effectiveVersion` 变化
  When UI 运行中
  Then 60s 内出现明确提示。
- AC2（Deploy update click）When 点击提示
  Then 页面刷新后前端成功更新（显示新 `webDistBuildId`），提示消失。
- AC3（Upstream release）Given GitHub latest release 版本高于当前 `effectiveVersion`
  When UI 加载
  Then 出现“新版本可用”提示并可跳转到 release 页面。
- AC4（Repo link）UI 可查看并打开仓库地址；版本号可见。
- AC5（Offline-safe）Given GitHub 不可达/限流
  Then UI 仍可用，更新检查显示失败原因（不崩溃、不刷屏）。

## 测试（Testing）

- Rust integration tests：
  - `/api/bootstrap` 包含 `app`。
  - `/api/meta` 返回期望字段。
  - `/api/update` 使用 stub GitHub server 验证更新判断逻辑。
- Web：`bun run lint`、`bun run typecheck`、`bun run build`。
