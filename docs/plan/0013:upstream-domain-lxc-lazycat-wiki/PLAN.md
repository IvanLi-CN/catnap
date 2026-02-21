# 上游站点域名迁移：lxc.lazycat.wiki（#0013）

## 状态

- Status: 已完成
- Created: 2026-02-21
- Last: 2026-02-21

## 背景 / 问题陈述

Catnap 需要抓取上游购物车页面（默认由 `CATNAP_UPSTREAM_CART_URL` 指定）。当前仓库内仍存在对“旧上游域名”的硬编码与示例文档，导致：

- 新部署不设置 env 时会抓取旧站点；
- README / deploy 示例会误导用户；
- 测试与 Storybook fixtures 的示例链接不一致。

## 目标 / 非目标

### Goals

- 将所有硬编码与示例中的旧上游地址迁移为 `https://lxc.lazycat.wiki/cart`（注意：`/cart` **不能带尾随斜杠**）。
- 保持行为不变：仅更新默认值/文档/fixtures，不改解析逻辑，不做多上游抽象。

### Non-goals

- 不变更上游解析器与选择器。
- 不引入新的配置项或 HTTP API。

## 范围（Scope）

### In scope

- Rust runtime 默认值：`src/config.rs` 的 `CATNAP_UPSTREAM_CART_URL` fallback。
- Deploy 示例：`deploy/compose.yaml`、`deploy/.env.example`。
- README 文案与示例 env。
- Rust integration tests 中的 `upstream_cart_url` 固定值。
- Web Storybook fixtures 中的 `siteBaseUrl` 示例域名。
- `docs/plan/**` 中历史文档出现的旧域名引用（保持全仓一致）。

### Out of scope

- 任何与上游站点结构变化相关的修复（若解析失败，另起计划/PR）。

## 验收标准（Acceptance Criteria）

1. 仓库中不再出现旧上游域名（默认不保留任何旧域名引用）。
2. `CATNAP_UPSTREAM_CART_URL` 默认值为 `https://lxc.lazycat.wiki/cart`。
3. README 与 deploy 示例展示的新默认值与上述一致，并提示 “`/cart` 无尾随 `/`”。
4. `cargo test --all-features` 通过。
5. `cd web && bun run lint` 与 `cd web && bun run typecheck` 通过。

## 风险

- 上游路径严格：`/cart/` 可能 404，因此必须使用 `https://lxc.lazycat.wiki/cart`（无尾随斜杠）作为默认入口。

## 落地记录

- PR: #44
