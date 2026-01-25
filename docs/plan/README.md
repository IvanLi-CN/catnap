# 计划（Plan）总览

本目录用于管理“先计划、后实现”的工作项：每个计划在这里冻结范围与验收标准，进入实现前先把口径对齐，避免边做边改导致失控。

## 快速新增一个计划

1. 分配一个新的四位编号 `ID`（查看下方 Index，取未使用的最小或递增编号）。
2. 新建目录：`docs/plan/<id>:<title>/`（`<title>` 用简短 slug，建议 kebab-case）。
3. 在该目录下创建 `PLAN.md`。
4. 在下方 Index 表新增一行，并把 `Status` 设为 `待设计` 或 `待实现`（取决于是否已冻结验收标准），并填入 `Last`（通常为当天）。

## 目录与命名规则

- 每个计划一个目录：`docs/plan/<id>:<title>/`
- `<id>`：四位数字（`0001`–`9999`），一经分配不要变更。
- `<title>`：短标题 slug（建议 kebab-case，避免空格与特殊字符）；目录名尽量稳定。
- 人类可读标题写在 Index 的 `Title` 列；标题变更优先改 `Title`，不强制改目录名。

## 状态（Status）说明

仅允许使用以下状态值：

- `待设计`：范围/约束/验收标准尚未冻结，仍在补齐信息与决策。
- `待实现`：计划已冻结，允许进入实现阶段（或进入 PM/DEV 交付流程）。
- `部分完成（x/y）`：实现进行中；`y` 为该计划里定义的里程碑数，`x` 为已完成里程碑数（见该计划 `PLAN.md` 的 Milestones）。
- `已完成`：该计划已完成（实现已落地或将随某个 PR 落地）；如需关联 PR 号，写在 Index 的 `Notes`（例如 `PR #123`）。
- `作废`：不再推进（取消/价值不足/外部条件变化）。
- `重新设计（#<id>）`：该计划被另一个计划取代；`#<id>` 指向新的计划编号。

## `Last` 字段约定（推进时间）

- `Last` 表示该计划**上一次“推进进度/口径”**的日期，用于快速发现长期未推进的计划。
- 仅在以下情况更新 `Last`（不要因为改措辞/排版就更新）：
  - `Status` 变化（例如 `待设计` → `待实现`，或 `部分完成（x/y）` → `已完成`）
  - `Notes` 中写入/更新 PR 号（例如 `PR #123`）
  - `PLAN.md` 的里程碑勾选变化
  - 范围/验收标准冻结或发生实质变更

## Index（固定表格）

| ID   | Title | Status | Plan | Last | Notes |
|-----:|-------|--------|------|------|-------|
| 0001 | 懒猫云购物车库存监控 | 已完成 | `0001:lazycats-cart-inventory-monitor/PLAN.md` | 2026-01-20 | UI 对齐 wireframes + 监控页重新同步（Playwright 复验） |
| 0002 | Storybook 展示与主题切换（含亮色主题） | 已完成 | `0002:storybook-theme-switching/PLAN.md` | 2026-01-20 | 补齐 stories 覆盖（components/pages/layout） |
| 0003 | CI/CD：发版自动化（GHCR + GitHub Release + Release Assets）对标与补齐 | 已完成 | `0003:release-automation-alignment/PLAN.md` | 2026-01-21 | Release assets（4 targets + sha256）+ GHCR multi-arch + PR smoke test + UI embed |
| 0004 | 配置卡片：库存历史与近 1 日走势（minute bucket） | 已完成 | `0004:inventory-history-trend/PLAN.md` | 2026-01-21 | 实现：history API（batch）+ 卡片背景 sparkline + 30 天清理；API=sparse；>10=10+ |
| 0005 | CI/CD：自动发版意图标签与版本号策略（防止 docs-only 发版） | 已完成 | `0005:release-intent-label-gating/PLAN.md` | 2026-01-21 | PR label gate：`type:docs|skip|patch|minor|major`；无关联 PR 的 `push main`=跳过；base=语义版本最大 tag（无 tag fallback `Cargo.toml`）；按标签 bump |
| 0006 | CI/CD：GitHub Actions 构建提速（PR：跳过 arm64 + gating + cache） | 已完成 | `0006:github-actions-performance/PLAN.md` | 2026-01-22 | PR ≤ 10 分钟（baseline Actions run `21235535755`）；PR smoke job 输出 timings summary（key steps） |
| 0007 | CI/CD：修复 Release job 失败（Determine release version/tag） | 部分完成（2/3） | `0007:release-job-unblock/PLAN.md` | 2026-01-24 | root cause：step 内运行 `compute-version.sh` 但变量未在同一 shell 可见（`set -u` unbound）；补齐：支持 `push tag` 路径；review：避免 bot tag push 重复 release + path-gate 误判；tag 校验收紧为 `v<semver>` |
| 0008 | 系统设置：通知测试按钮（Telegram + Web Push） | 待实现 | `0008:settings-notifications-test-button/PLAN.md` | 2026-01-23 | Telegram：可用已保存配置或临时覆盖（不保存）；Web Push：补齐发送链路（VAPID private/subject）用于测试 |
| 0009 | 全量刷新：SSE 进度 + 缓存复用 + 配置上下架 | 待实现 | `0009:catalog-full-refresh-sse/PLAN.md` | 2026-01-24 | 全局调度=最小间隔；缺失一次即下架；监控页展示最近 24h 上架（含重新上架） |
| 0010 | 配置卡片：国家国旗水印背景 | 待实现 | `0010:card-country-flag-watermark/PLAN.md` | 2026-01-25 | 组件设计图已确认（`docs/ui/cards.svg`） |
| 0011 | 采集观测台：全局采集队列 + SSE 日志订阅 | 待实现 | `0011:ops-collection-dashboard/PLAN.md` | 2026-01-25 | SSE 断线续传（1h）+ 7d 留存 + range=24h/7d/30d |
