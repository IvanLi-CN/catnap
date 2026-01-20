# Storybook 展示与主题切换（含亮色主题）（#0002）

## 状态

- Status: 待实现
- Created: 2026-01-20
- Last: 2026-01-20

## 背景 / 问题陈述

- `web/` 目前是 React + Vite 的最小应用，样式为硬编码深色，缺少可复用的主题系统与切换入口。
- 缺少 Storybook，导致布局/页面/组件无法系统化展示、分组复用与做 story-based 自动化测试。
- 本计划为后续 Web UI 开发（例如计划 #0001）补齐：Storybook 基建、亮色主题设计、主题切换与 Storybook 主题控制（与 Web 内状态对齐）。

## 目标 / 非目标

### Goals

- 在 `web/` 引入 Storybook（面向 React + Vite），并建立可持续的 stories 分组与覆盖口径：Layout / Pages / Components（含代表性状态）。
- 设计并落地 Web UI 亮/暗两套主题 token，并提供**三态**主题偏好：`system` / `dark` / `light`（默认 `system`），含主题切换入口与持久化。
- 在 Storybook 提供主题切换（`system` / `dark` / `light`），并与 Web 内主题系统共享同一套契约（详见 `contracts/ui-theme.md`），采用**单向同步**。
- 引入 story-based 自动化测试（采用 Vite builder 推荐路径：`@storybook/addon-vitest`），并接入现有 CI（不引入第三方 SaaS）。

### Non-goals

- 不在本计划内实现具体业务页面与组件（业务功能交付以 #0001 及后续计划为准）。
- 不引入/替换 UI 框架（如 Tailwind/MUI），不做大规模样式重构；仅建立主题 token 与切换机制。
- 不引入第三方视觉回归 SaaS（如 Chromatic）；若需要另起计划。

## 用户与场景（Users & Scenarios）

- Web 开发者：在 Storybook 中查看组件/页面/布局的不同状态，调试交互与视觉。
- 评审/验收：通过 Storybook 快速预览页面与关键交互，减少“跑起整站才能看”的成本。
- CI：运行 story-based 测试，防止主题/布局/组件回归。

## 需求（Requirements）

### MUST

- Storybook 基建
  - 必须在 `web/` 引入 Storybook，并提供本地开发命令与静态构建命令（见 `contracts/cli.md`）。
  - 必须在 Storybook 中建立分组（至少：Foundations/Theme、Layout、Pages、Components）。
  - 必须提供“覆盖口径”：哪些目录/哪些导出需要配套 stories；并要求“代表性状态”最少覆盖（见下方验收与契约）。

- 亮色主题与主题切换（Web）
  - 必须支持 `system`/`dark`/`light` 三态主题偏好（`system` 跟随 `prefers-color-scheme`）。
  - 必须提供 `dark` 与 `light` 两套主题 token（CSS variables），并在 Web 内提供**三态菜单**主题切换入口（`system/dark/light`）。
  - 必须持久化主题偏好，并在刷新/重启后保持一致（契约见 `contracts/ui-theme.md`）。
  - 必须保证两主题下的可读性与基本对比度（尤其是正文/次要文本/错误态）。

- 主题切换（Storybook）与同步
  - 必须在 Storybook 提供主题切换（toolbar 或等价入口），至少包含 `system/dark/light`。
  - 必须定义“同步规则”：**单向同步**（Storybook 控制 → 预览一致），契约见 `contracts/ui-theme.md`。

- story-based 自动化测试
  - 必须接入 Storybook 的测试能力，并在 CI 中可运行（至少覆盖：主题切换相关 story + 若干关键组件的代表性状态）。
  - 必须采用 Vite builder 推荐路径：`@storybook/addon-vitest`（不引入 `@storybook/test-runner`）。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Web 主题系统（Theme state + tokens + DOM/persistence） | UI Component | internal | New | ./contracts/ui-theme.md | FE | Web UI / Storybook / Tests | `data-theme` + CSS variables |
| Web 命令（Storybook / tests scripts） | CLI | internal | Modify | ./contracts/cli.md | FE | Dev / CI | `web/package.json` scripts |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/ui-theme.md](./contracts/ui-theme.md)
- [contracts/cli.md](./contracts/cli.md)

## 约束与风险（Constraints & Risks）

- `web/` 当前使用 Bun；Storybook 初始化工具对 Bun 的支持不足：本计划固定采用 **Bun-only 手动落地配置**（参考过往项目的 `.storybook/*` 形状），不依赖 `storybook init/create`。
- 即便采用 `@storybook/addon-vitest`，story-based 测试仍是浏览器模式（Playwright provider），需要在 CI 中安装/缓存，可能增加 CI 时间。
- 主题从“硬编码深色”演进到“token + 切换”，需要避免对既有页面造成不可控样式漂移（建议分阶段落地）。

## 验收标准（Acceptance Criteria）

- Given 在 `web/` 完成依赖安装
  When 执行 `bun run storybook`
  Then Storybook 可启动并展示分组（Foundations/Theme、Layout、Pages、Components）。

- Given Storybook 预览中包含主题示例（Foundations/Theme）
  When 在 Storybook 中切换主题为 `light`
  Then 预览区根节点应用 `data-theme="light"` 且视觉为亮色主题；切回 `dark` 同理；切换到 `system` 时应移除 `data-theme` 并跟随 `prefers-color-scheme`（按 `contracts/ui-theme.md`）。

- Given 浏览器首次访问（`localStorage['catnap.theme']` 不存在）
  When 打开 Web UI
  Then 默认主题偏好为 `system`（不写入 `data-theme`），并随 `prefers-color-scheme` 生效（按 `contracts/ui-theme.md`）。

- Given Web UI 的主题切换入口已实现
  When 用户在 Web 中切换主题并刷新页面
  Then 主题偏好被持久化，并在刷新后保持一致（持久化与 DOM 规则符合 `contracts/ui-theme.md`）。

- Given story-based 自动化测试已接入 CI
  When CI 运行 `bun run test:storybook`
  Then 若任一关键 story 的交互/断言失败则 CI 失败；成功时 CI 通过。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Storybook tests：至少覆盖主题切换相关 story，以及若干关键组件的代表性状态（loading/empty/error/disabled 等）。
- Unit tests（如适用）：主题工具函数（解析/持久化/应用 DOM）的纯逻辑可用单测覆盖（可参考过往项目的 `theme.test.ts` 结构）。

### Quality checks

- `web/`：`bun run lint`、`bun run typecheck`、`bun run build` 通过。
- Storybook：静态构建可在 CI 中通过（例如 `bun run build-storybook`）。

## 文档更新（Docs to Update）

- `web/README.md`：补充 Storybook 使用方式、主题切换口径、story 组织/命名与测试运行方式。
- `docs/plan/0001:lazycats-cart-inventory-monitor/PLAN.md`（如需要）：在实现阶段把“Storybook/主题系统”为前端工作项的依赖关系写清楚，避免计划漂移。

## 实现里程碑（Milestones）

- [ ] M1: 在 `web/` 初始化 Storybook（Bun-only：手动落地 `.storybook/*` + scripts；含基础分组与最小示例 stories）
- [ ] M2: 落地主题 token（dark/light）与 Web 主题切换入口（含 system 支持与持久化）
- [ ] M3: Storybook 主题切换接入并与 Web 主题状态对齐（按 `contracts/ui-theme.md`）
- [ ] M4: 接入 story-based 自动化测试并接入 CI（含最小覆盖）

## 方案概述（Approach, high-level）

本计划的实现方案**已冻结**，以你的过往工程实践为蓝本：

- 主题系统（参考 `isolapurr-usb-hub`）：
  - 主题状态：`localStorage['catnap.theme']`（JSON string）为单一真相来源；`system` 时移除 `data-theme` 并用 media query 跟随系统。
  - DOM：`document.documentElement` 写入/移除 `data-theme`，并同步 `color-scheme`（规则见 `contracts/ui-theme.md`）。
  - UI：提供三态菜单 `system/dark/light`（行为见 `contracts/ui-theme.md`）。
- Storybook（参考 `dockrev` 的 toolbar + decorator 形态）：
  - 使用 `.storybook/preview.ts(x)` 定义 `globalTypes.theme`（items: `system/dark/light`），并在 decorator 内按选中值写入 `localStorage['catnap.theme']` + 应用到 `data-theme`。
  - 单向同步：仅 Storybook toolbar → 预览一致。
- 测试（按 Storybook 对 Vite builder 的推荐路径）：
  - 采用 `@storybook/addon-vitest` + `vitest --project=storybook`，并启用浏览器模式（Playwright provider）。
  - 交互断言写在 stories 的 `play` 中，使用 `storybook/test`（`expect/within/userEvent/waitFor/fn`）。

### 落地清单（Implementation footprint）

实现阶段按下列“落点”落地（与 `contracts/*.md` 一致）：

- `web/package.json`
  - scripts：`storybook` / `storybook:ci` / `build-storybook` / `test:storybook`（见 `contracts/cli.md`）
  - devDependencies（固定方向）：
    - Storybook: `storybook` + `@storybook/react-vite` + `@storybook/addon-essentials` + `@storybook/addon-vitest`
    - Tests: `vitest` + `@vitest/browser-playwright` + `playwright`（需要安装 Playwright 浏览器）
- `web/.storybook/main.ts`
  - `framework: "@storybook/react-vite"`；`addons: ["@storybook/addon-essentials", "@storybook/addon-vitest"]`
  - `stories` glob 覆盖：`../src/**/*.stories.@(ts|tsx)`（可选含 mdx）
  - `core.disableTelemetry = true`
- `web/.storybook/preview.ts(x)`
  - `globalTypes.theme`：`system/dark/light`
  - `decorators`：按选中主题写入 `localStorage['catnap.theme']` 并应用到 `data-theme` + `color-scheme`（见 `contracts/ui-theme.md`）
- `web/.storybook/vitest.setup.ts`
  - `setProjectAnnotations([previewAnnotations])`（按 Storybook 文档；不做双向同步）
- `web/vitest.config.ts`
  - `storybookTest({ configDir: ".storybook", storybookScript: "bun run storybook:ci" })`
  - `test.name = "storybook"`，以支持 `vitest --project=storybook`
  - `test.browser` 启用浏览器模式（Playwright provider：`playwright({})`）
  - `setupFiles: ["./.storybook/vitest.setup.ts"]`
- `web/src/app/theme.ts`（文件名可按仓库约定微调，但契约不变）
  - `load/save/apply/init` 函数，行为与 `contracts/ui-theme.md` 一致
- `web/src/ui/nav/ThemeMenu.tsx`（或等价位置）
  - 三态菜单 `system/dark/light`，写入并应用主题（见 `contracts/ui-theme.md`）
- `web/src/app.css`
  - 将现有“硬编码深色”替换为 token（`--color-*`），并补齐 light/dark 两套值 + system 跟随逻辑（见 `contracts/ui-theme.md`）
- `web/src/**` stories（最小必备）
  - `Foundations/Theme`：展示 token 与主题切换效果（system/dark/light）
  - `Layout/AppShell`：展示基础布局骨架（可先用当前 `App`/占位布局）
  - `Pages/App`：展示当前页面级组件（为后续 pages 扩展预留分组）
  - `Components/ThemeMenu`：覆盖三态切换（含代表性状态）

## 开放问题（需要主人回答）

None.

## 假设（Assumptions）

None.

## 变更记录（Change log）

- 2026-01-20: 创建计划 #0002，补齐目标/范围/契约与验收草案。
- 2026-01-20: 冻结方案：`system/dark/light` 三态主题（默认 system）、单向同步、Bun-only 手动配置、`@storybook/addon-vitest` 测试路径。

## 参考（References）

- Storybook 安装（React + Vite）与 CLI：`https://storybook.js.org/docs/get-started/install`
- Themes（data attribute decorator）：`https://storybook.js.org/docs/essentials/themes#use-the-data-attribute-decorator`
- Toolbars & globals：`https://storybook.js.org/docs/essentials/toolbars-and-globals`
- Vitest addon：`https://storybook.js.org/docs/writing-tests/integrations/vitest-addon`
- 本地参考（Ivan 过往项目）：
  - `isolapurr-usb-hub/web/src/app/theme.ts`（三态主题偏好与持久化形状）
  - `isolapurr-usb-hub/web/src/index.css`（`data-theme` + `prefers-color-scheme` 的 token 组织方式）
  - `dockrev/web/.storybook/preview.ts`（toolbar globals + decorator 切主题的写法）
