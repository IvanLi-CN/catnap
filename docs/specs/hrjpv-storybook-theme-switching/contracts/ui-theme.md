# UI 主题系统（Theme state + tokens）

本文件冻结 Web 与 Storybook 共用的主题“状态形状 + DOM 落点 + 持久化规则 + token 命名”。

## Theme（`ThemeMode`）

- 范围（Scope）: internal
- 变更（Change）: New

### 状态（State）

- `ThemeMode`: `system | dark | light`

### 持久化（Persistence）

- Storage: `localStorage`
- Key: `catnap.theme`
- Value: `system` / `dark` / `light`
- Encoding: JSON string（例如 `"system"`），与过往项目保持一致（便于校验与容错）
- 默认值：
  - 若 key 不存在：默认 `system`

### DOM 落点（DOM binding）

- 目标元素：`document.documentElement`（即 `<html>`）
- Data attribute：
  - name: `data-theme`
  - values: `dark` / `light`
  - `system` 模式：移除 `data-theme`（`<html>` 不应包含该 attribute）
- `color-scheme`（固定口径）：
  - `system`：`document.documentElement.style.colorScheme = "light dark"`
  - `dark`：`document.documentElement.style.colorScheme = "dark"`
  - `light`：`document.documentElement.style.colorScheme = "light"`

## ThemeMenu（UI 入口）

- 范围（Scope）: internal
- 变更（Change）: New

### 行为（Behavior）

- UI 入口为**三态菜单**：`system` / `dark` / `light`
- 选择某一项时：
  - 写入 `localStorage['catnap.theme']`（按本文件 Encoding）
  - 立即应用到 `document.documentElement`（设置/移除 `data-theme`，并设置合适的 `color-scheme`）

### 同步规则（Web ↔ Storybook）

#### Source of truth（单一真相来源，已冻结）

- **localStorage 为准**：读取 `localStorage['catnap.theme']`，并将其应用到 DOM attribute（按本文件约定）。

#### 最小要求（必须满足）

- Storybook 的主题切换必须驱动 `data-theme`（从而驱动 token 生效）。
- Web 内主题切换入口必须更新 `data-theme` 与持久化（按上面的规则）。
- Storybook 与 Web 的“默认主题”必须一致（避免预览与页面初始不一致）。
  - 本计划采用 **单向同步**：Storybook toolbar 控制预览主题；Web 内切换入口不要求反向更新 toolbar 状态。

## Theme tokens（CSS variables）

> 本节冻结“token 命名”；具体颜色值在实现阶段可迭代，但需要保证语义一致与可读性。

### 必备 tokens（MUST）

- `--color-bg`：页面背景
- `--color-fg`：主文本
- `--color-fg-muted`：次要文本
- `--color-surface-1`：一级容器（卡片/面板）
- `--color-surface-2`：二级容器（内嵌块）
- `--color-border`：分隔线/边框
- `--color-accent`：主强调色（按钮/高亮）
- `--color-danger`：错误色

### CSS 组织方式（建议）

- `web/src/app.css`（或后续的 theme 文件）中使用如下结构（示例为形状，不冻结具体值）：
  - `:root { ...light tokens... }`（作为 system/light 的基底）
  - `:root[data-theme="dark"] { ...dark tokens... }`（强制 dark）
  - `:root[data-theme="light"] { ...light tokens... }`（强制 light，可与 base 相同或更明确）
  - `@media (prefers-color-scheme: dark) { :root:not([data-theme]) { ...dark tokens... } }`（system + OS=dark）

## Storybook integration（Preview）

Storybook 侧建议使用 toolbar globals + preview decorator（可参考过往项目 `dockrev/web/.storybook/preview.ts` 的写法），以支持 `system/dark/light` 三态：

- Globals: `theme = system | dark | light`
- Decorator: 根据 `theme` 写入 `localStorage['catnap.theme']`（或直接调用共享的 apply 函数）并设置/移除 `data-theme`

> 若后续需要“Web 内切换 → 反向更新 Storybook toolbar”（双向同步），需要额外定义事件/回调机制；不在本计划的 MUST 范围内。
