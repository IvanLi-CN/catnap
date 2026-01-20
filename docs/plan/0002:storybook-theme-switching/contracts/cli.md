# 命令行（CLI）

每个命令一个小节；保持短小但可实现、可测试。

## Web: Storybook scripts（`web/package.json`）

- 范围（Scope）: internal
- 变更（Change）: Modify

### 用法（Usage）

```text
# Storybook dev server
bun run storybook

# Storybook dev server (CI-friendly: no browser auto-open)
bun run storybook:ci

# Build static Storybook site
bun run build-storybook

# Run story-based tests (CI)
bun run test:storybook
```

### 参数（Args / options）

本计划冻结 `web/package.json` 的 scripts 口径（实现阶段按此落地）：

- `storybook`: `storybook dev`
- `storybook:ci`: `storybook dev --ci --no-open`
- `build-storybook`: `storybook build`
- `test:storybook`: `vitest --project=storybook`

> 说明：如后续需要固定端口/输出目录，应在 scripts 中显式写出，并同步更新本文件与 `PLAN.md` 的验收标准。

### 输出（Output）

- `bun run storybook`:
  - Format: human（终端日志）
  - Runs a local server（默认端口通常为 `6006`，以实际配置为准）
- `bun run storybook:ci`:
  - Format: human（终端日志）
  - Runs a local server（不自动打开浏览器；用于测试启动）
- `bun run build-storybook`:
  - Format: human（终端日志）
  - 输出目录：`storybook-static/`（默认；以实际配置为准）
- `bun run test:storybook`:
  - Format: human（终端日志）
  - 固定实现：采用 `@storybook/addon-vitest`（Vite builder 推荐路径），执行 `vitest --project=storybook`；由 Vitest plugin 按 `storybookScript` 启动 `storybook:ci` 并在浏览器模式下运行（Playwright provider）。
  - 失败时应输出失败的 story/test 信息，便于定位。
  - Dependencies（固定方向）：`vitest` + `@vitest/browser-playwright` + `playwright`

### 退出码（Exit codes）

- `0`: 成功
- `1`: 失败（lint/test/build failure）

### 兼容性与迁移（Compatibility / migration）

- 若后续将包管理器从 Bun 切换到 pnpm/npm，需要同时更新：`web/package.json` scripts、CI 中的 install/run 命令，以及本契约文档。
