# 关于：版本号显示 + 升级提示 + 仓库地址显示（#0012）

## 状态

- Status: 已完成
- Created: 2026-02-17
- Last: 2026-02-17

## 背景 / 问题陈述

当前 Catnap 已具备稳定的发版口径（`APP_EFFECTIVE_VERSION`）与探活接口（`GET /api/health`），但 UI 缺少三个运维常用信息：

- 当前运行版本号（便于排障、确认发布是否生效）
- 仓库地址（便于快速定位源码、Release、文档）
- 升级提示（当 GitHub Releases 有新 stable 版本时提示维护者）

我们希望在不影响核心业务（库存监控/通知/刷新）的前提下补齐这一“关于（About）”能力：可见、可解释、失败不打断。

## 目标 / 非目标

### Goals

- UI 中常驻显示“当前版本号”与“仓库地址”。
- 自动检查 GitHub Releases 的 stable latest，发现新版本时给出“升级提示”与跳转链接。
- 升级检查具备缓存（TTL + ETag）与超时控制，避免频繁外网请求；失败时降级展示，不影响主功能。
- 不改动现有 `GET /api/bootstrap` 的响应 shape（避免破坏既有契约与测试）。

### Non-goals

- 不实现自我升级（不负责拉镜像、替换容器、重启服务）。
- 不展示 commit SHA（保持 semver 口径为准）。
- 不将升级检查做成强依赖（离线/被墙/限流时仍可使用 Catnap）。

## 范围（Scope）

### In scope

- Backend：
  - 新增 `GET /api/about`（internal）：返回版本号、仓库地址、web dist build id、升级检查结果。
  - GitHub Releases stable latest 检查：TTL 缓存 + ETag（304）+ 超时 + 失败降级。
- Frontend：
  - 侧边栏底部 meta：版本号 + 仓库链接（常驻可见）。
  - 系统设置页：关于/更新信息（当前版本/最新版本/检查时间/错误信息）+ “检查更新”按钮（force）。
  - 顶栏 actions：发现新版本时显示轻量提示 pill（不阻断）。

### Out of scope

- 任何自动更新执行器与回滚机制。

## 需求（Requirements）

### MUST

- `GET /api/about` 返回结构固定、可前端直接渲染（见 `contracts/http-apis.md`）。
- 当前版本号必须来自运行中服务的 `effective_version`（`APP_EFFECTIVE_VERSION` → fallback `CARGO_PKG_VERSION`）。
- 仓库地址必须可配置（默认官方仓库；fork/私有部署可覆盖）。
- 升级检查：
  - 只对比 GitHub Releases 的 stable latest（不含 prerelease）。
  - 默认自动检查 + 缓存（TTL），设置页可强制刷新（`force=1`）。
  - 外网请求必须有短超时；失败不影响主功能，且保留 last-known good（如存在）。

## 接口契约（Interfaces & Contracts）

- HTTP API：`GET /api/about` → `contracts/http-apis.md`
- Config：新增 env vars → `contracts/config.md`

## 验收标准（Acceptance Criteria）

- Given 打开 UI
  When 页面加载完成
  Then 侧边栏底部可见当前版本号与仓库链接

- Given GitHub Releases 有新 stable 版本 `vX.Y.Z` 且 `X.Y.Z > current`
  When 页面加载完成
  Then 顶栏出现“有新版本”提示，且设置页可看到 latest 版本与跳转链接

- Given 运行环境离线或 GitHub API 请求失败
  When 页面加载完成
  Then UI 仍可正常使用；设置页显示“无法检查更新”的短错误信息（如存在）

- Given 点击设置页“检查更新”
  When 请求 `GET /api/about?force=1`
  Then 缓存被强制刷新，并更新 `checkedAt`（无论成功/失败）

## 非功能性验收 / 质量门槛（Quality Gates）

- Rust：
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test --all-features`
- Web：
  - `cd web && bun run lint`
  - `cd web && bun run typecheck`

## 实现里程碑（Milestones）

- [x] M1: docs（本计划 + 契约 + Index）
- [x] M2: 后端 `GET /api/about` + 缓存 + 配置
- [x] M3: 前端侧边栏 meta + 设置页 about/update + 顶栏提示
- [x] M4: tests + lint/typecheck 全绿
