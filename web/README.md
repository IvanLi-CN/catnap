# Catnap Web

`web/` 是 Catnap 的前端（React + Vite + Bun）。

## 开发

```bash
cd web
bun install
bun run dev
```

## Storybook

```bash
# dev server
bun run storybook

# dev server (CI-friendly: no browser auto-open)
bun run storybook:ci

# static build
bun run build-storybook
```

默认固定端口：`18181`（避免使用默认端口导致冲突）。

### Stories 分组与覆盖口径

- Foundations/Theme：主题 tokens 与主题切换效果
- Layout/*：布局骨架（不依赖真实 API）
- Pages/*：页面级组件（用 stub 展示结构与状态，不跑真实 API）
- Components/*：可复用组件与代表性状态（loading/empty/error/disabled 等）

约定：

- stories 文件统一放在 `src/**` 下，使用 `*.stories.tsx` 命名。
- 新增 `src/ui/**` 的可复用组件时，必须配套 story（至少 1 个默认状态 + 若干代表性状态）。

## 主题系统（system / dark / light）

- ThemeMode：`system | dark | light`
- Storage：`localStorage["catnap.theme"]`（JSON string，例如 `"dark"`）
- DOM：`document.documentElement` 写入/移除 `data-theme="dark|light"`；`system` 模式应移除 `data-theme`
- `color-scheme`：
  - system：`light dark`
  - dark：`dark`
  - light：`light`

实现落点：

- `src/app/theme.ts`：load/save/apply/init
- `src/app.css`：`--color-*` tokens + `data-theme`/`prefers-color-scheme` 组织
- `src/ui/nav/ThemeMenu.tsx`：三态菜单入口

## story-based tests（Vitest + Playwright）

```bash
cd web

# install browser binaries (first time / CI)
bunx playwright install chromium

# run tests (storybook addon vitest)
bun run test:storybook
```
