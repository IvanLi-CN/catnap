# 移动端响应式适配与 Storybook 全断点 DOM 验收（#pc6du）

## 状态

- Status: 部分完成（4/5）
- Created: 2026-03-04
- Last: 2026-03-04

## 背景 / 问题陈述

当前 Web UI 以桌面布局为主，缺少移动端（小屏幕）布局与断点合同，Storybook 中也没有覆盖“全页面 × 全断点”的 DOM 验收。结果是：

- 小屏下导航与筛选区域存在可用性风险；
- 固定宽度控件在窄屏可能产生横向溢出；
- 没有统一断点清单与自动化验收矩阵，回归难以及时发现。

## 目标 / 非目标

### Goals

- 明确并落地支持宽度范围：`360px - 1680px`（大于 `1680px` 复用上限布局）。
- 建立断点合同：`360-479`、`480-767`、`768-1023`、`1024-1219`、`1220-1439`、`1440-1680`。
- App Shell 在 `<=1023` 提供汉堡抽屉导航（含遮罩/关闭逻辑/可访问性属性）。
- 完成 Monitoring / Products / Settings / Logs / Ops 五个页面的断点兼容，避免横向溢出。
- 在 Storybook 明确列出全部断点，并提供全页面 DOM 自动化验收。

### Non-goals

- 不修改 Rust 后端 API 与数据结构。
- 不新增业务流程，仅做布局与交互适配。
- 不重做视觉风格，仅在既有设计语言上完成响应式兼容。

## 范围（Scope）

### In scope

- `web/src/app.css`：统一断点样式（含 container query 断点、页面布局与组件宽度适配）。
- `web/src/ui/layout/AppShell.tsx`：移动端抽屉导航 + 稳定测试锚点（`data-testid`）。
- `web/src/App.tsx`：页面根节点 `data-testid`，供全页面 DOM 验收。
- Storybook：
  - `.storybook/preview.tsx`：断点预设工具栏；
  - `src/stories/breakpoints.ts`：断点单一事实源；
  - 页面 stories 增加 `ResponsiveAllBreakpoints` + `play` DOM 断言；
  - 新增 `Foundations/ViewportBreakpoints` 展示断点合同。

### Out of scope

- 后端服务逻辑、数据库迁移、采集策略调整。
- 新增非必要 UI 功能点。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `AppShell` 新增移动端导航行为 | React component behavior | internal | Modify | None | web | web stories/tests | `<=1023` 抽屉导航；桌面保留侧栏 |
| `data-testid`（AppShell 与五个页面根节点） | DOM contract | internal | Add | None | web | storybook tests | 用于断点 DOM 验收 |
| `RESPONSIVE_BREAKPOINTS` | TS constant | internal | Add | None | web | preview + stories + tests | 断点单一事实源 |

## 验收标准（Acceptance Criteria）

- Given 视口宽度在 `360..1680`
  When 打开任一路由页面
  Then 页面主容器无横向溢出（`scrollWidth <= clientWidth + 1`）。

- Given 视口宽度 `<=1023`
  When 用户点击顶部汉堡按钮
  Then 抽屉导航可打开并可通过遮罩关闭。

- Given 视口宽度 `>=1024`
  When 打开页面
  Then 保持桌面侧栏导航，汉堡按钮不显示。

- Given Storybook
  When 查看 Foundations 与页面 stories
  Then 可见全部断点列表，并能运行全页面断点 DOM 自动化用例。

- Given 5 个页面 × 7 个视口点
  When 执行 `bun run test:storybook`
  Then 35 个断点场景全部通过。

## 非功能性验收 / 质量门槛（Quality Gates）

- `cd web && bun run lint`
- `cd web && bun run typecheck`
- `cd web && bun run test:storybook`
- `cargo test --all-features`

## 实现里程碑（Milestones）

- [x] M1: 断点合同与 Storybook 断点预设落地（breakpoint 单一事实源 + toolbar）
- [x] M2: App Shell 小屏抽屉导航实现（含遮罩关闭与 Esc 关闭）
- [x] M3: 五个主页面小屏兼容（无横向溢出）
- [x] M4: 全页面全断点 DOM 自动化验收（5×7）接入 Storybook tests
- [ ] M5: PR + checks + review-loop 收敛并回填索引状态

## 风险 / 假设

- 假设：浏览器环境支持 container query（现代 Chromium 已支持）。
- 风险：大量窄屏样式覆盖可能影响极端内容长度下的视觉密度，需要后续真实数据观察。

## 变更记录（Change log）

- 2026-03-04: 创建规格，冻结支持范围、断点合同与验收矩阵。
- 2026-03-04: 完成 App Shell 抽屉导航、页面适配、Storybook 断点预设与全页面 35 场景 DOM 测试。

