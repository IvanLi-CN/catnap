# 上架通知改为“有库存再发”（#fswrs）

## 状态

- Status: 部分完成（3/4）
- Created: 2026-03-11
- Last: 2026-03-11

## 背景 / 问题陈述

- 当前 `listed` 生命周期事件一旦配置进入 `active` 就会立即触发通知与 `catalog.listed` 日志，即使库存仍为 `0`。
- 线上数据已经证明这会放大“改名 / re-key / 空库存重上架”噪声：用户收到“新上架”，但实际并没有可下单库存。
- 同时，已加入单独监控的配置在 `0 -> >0` 时本来就会触发补货；若再额外补发 `listed`，同一轮会形成双重提醒。

## 目标 / 非目标

### Goals

- 将 `listed` 对外通知与 `catalog.listed` 用户日志改成“当前 active 生命周期内首次库存 `> 0` 时才触发”。
- 当配置先以库存 `0` 上架时，保留用户可见细节日志，说明该项已上架但仍在等待库存。
- 对已单独监控该配置的用户，在第一次 `0 -> >0` 时只保留现有补货提醒，不再同时收到 `listed`。
- 保持生命周期状态、`recentListed24h` 与下架通知语义不变，避免把“通知口径”误扩展成“数据口径”变更。

### Non-goals

- 不调整 `recentListed24h` 的筛选规则；它仍表示最近 24 小时发生 lifecycle listed（含 relisted）的配置。
- 不在本计划中解决改名 / re-key 被识别成 listed + delisted 的问题。
- 不修改 Logs API、settings API 或监控配置模型的公开字段结构。
- 不改变 `delisted`、价格变动、配置变动、补货的现有定义与文案。

## 范围（Scope）

### In scope

- `catalog_configs` 新增“本生命周期 listed 已对外发出”的持久化时间字段，用于幂等控制。
- `apply_catalog_url_result` / lifecycle fanout 拆分出：
  - 已 listed 但库存 `0`、仅记录细节的集合。
  - 已满足首次有库存、允许写 `catalog.listed` 并发通知的集合。
- 新增用户可见日志 scope `catalog.listed.pending_stock`，并同步写入 ops log。
- 设置页“上架监控”说明文案改成“首次有库存后通知”。
- 回归测试覆盖首次上架有库存、先零库存后补货、已监控用户去重分流等关键路径。

### Out of scope

- 监控页 `recentListed24h`、产品页生命周期展示与归档逻辑。
- Telegram / Web Push 渠道本身的发送策略、错误处理或限流机制。
- 新增额外设置项（例如“允许零库存 listed 通知”）。

## 需求（Requirements）

### MUST

- 当配置首次进入 `active` 且库存 `> 0` 时，启用上架监控且未监控该配置的用户应立即收到 `listed` 通知，并写入 `catalog.listed`。
- 当配置首次进入 `active` 但库存 `= 0` 时，不发送 `listed` 通知，不写 `catalog.listed`；必须写入 `catalog.listed.pending_stock` 说明该配置处于“已上架待库存”状态。
- 上述配置在同一 active 生命周期中第一次从 `0 -> >0` 时，最多补发一次 `listed`；之后库存再次归零再恢复，不再重复触发 `listed`。
- 若用户已在 `monitoring_configs` 中启用该配置，则该用户在第一次 `0 -> >0` 时只保留现有补货通知；该用户不再额外收到 `listed`。
- `recentListed24h` 继续按 `lifecycle_listed_at` 计算，不受新门槛影响。
- 历史上已经处于 `active` 的记录在迁移后必须被视为“listed 已经发过”，避免上线后对旧数据补发一轮 `listed`。

## 功能与行为规格（Functional/Behavior Spec）

### 生命周期与通知状态

- `lifecycle_state = active`：数据口径不变，仍在 apply 阶段按 upstream 结果维护。
- `lifecycle_listed_at`：继续表示最近一次从 `delisted -> active` 的时间，供 `recentListed24h` 使用。
- `lifecycle_listed_event_at`（新增）：表示本 active 生命周期内何时首次满足“可对外发出 listed”；
  - 新 listed 且库存 `> 0`：在同一轮 apply 中写入当前时间。
  - 新 listed 且库存 `= 0`：保持 `NULL`，直到未来第一次 `0 -> >0`。
  - relisted：进入新 lifecycle 时重置为 `NULL`，再按上述规则决定何时填充。
  - delisted：不需要保留旧值语义，进入下架时可置空或等待下次 active 覆盖，但不得影响下一轮 relisted 的幂等。

### 日志与通知分流

- `catalog.listed.pending_stock`
  - 面向启用上架监控的用户可见。
  - 文案应明确是“已上架，但当前库存 0，暂不通知”。
  - 需要同步写入 ops log，便于观测台追踪为何本轮没有真正发出 listed。
- `catalog.listed`
  - 仅在 `lifecycle_listed_event_at` 从 `NULL` 首次落值时产生。
  - 对“未监控该配置、但启用了上架监控”的用户发送 Telegram / Web Push。
  - 对“已监控该配置”的用户跳过 listed，以避免与补货提醒重复。
- `restock`
  - 现有 `poller` 逻辑保持不变。
  - 当某用户已监控该配置且该配置完成第一次 `0 -> >0` 时，该用户继续只收到补货提醒。

### 代表性场景

- 首次抓到配置 A，库存 `3`
  - 结果：立即 listed + `catalog.listed` + 外部通知。
- 首次抓到配置 B，库存 `0`
  - 结果：写 `catalog.listed.pending_stock`，不 listed。
- 配置 B 下一轮变成库存 `2`
  - 未监控该配置的 listed 用户：收到一次 listed。
  - 已监控该配置的用户：只收到补货，不再额外收到 listed。
- 配置 B 随后库存 `2 -> 0 -> 5`
  - 不再补发 listed；已监控用户仍可继续按现有补货语义收到补货。
- 配置 B 下架后再次 relisted 且库存 `0`
  - 进入新的 pending_stock 生命周期，直到首次库存 `> 0` 再重新允许 listed。

## 接口契约（Interfaces & Contracts）

### Public APIs

- 无公开 HTTP API 结构变更。
- `GET /api/monitoring` 中的 `recentListed24h`、`GET /api/logs` 的响应 shape 保持不变。

### Internal contracts

- `catalog_configs` 需要新增 `lifecycle_listed_event_at TEXT NULL`。
- `ApplyCatalogUrlResult` 需要能区分：
  - `listed_pending_zero_stock_ids`
  - `listed_event_ids`
  - `delisted_ids`
- lifecycle fanout 需要按用户是否存在启用中的 `monitoring_configs` 做 listed/retry 分流。

## 验收标准（Acceptance Criteria）

- Given 新配置首次进入 `active` 且库存 `> 0`
  When 本轮 apply 完成
  Then 启用上架监控且未监控该配置的用户收到 listed，日志包含 `catalog.listed`。

- Given 新配置首次进入 `active` 且库存 `= 0`
  When 本轮 apply 完成
  Then 不会发送 listed，也不会写 `catalog.listed`；但会写 `catalog.listed.pending_stock` 详情日志。

- Given 上述零库存配置之后第一次 `0 -> >0`
  When 该 active 生命周期仍未 delisted
  Then 只补发一次 listed；之后同 lifecycle 内不再重复。

- Given 某用户已单独监控该配置
  When 首次 `0 -> >0`
  Then 该用户只收到补货，不会同时再收到 listed。

- Given 历史 active 数据在迁移前已存在
  When 部署新版本并启动迁移
  Then 不会因为 `lifecycle_listed_event_at` 初始为空而对历史 active 项补发 listed。

- Given `recentListed24h` 查询
  When 某配置 relisted 但仍库存 `0`
  Then 该配置仍会按既有生命周期规则进入 `recentListed24h`，直到超过 24 小时或再次下架。

## 实现前置条件（Definition of Ready / Preconditions）

- 已确认：上架通知门槛只影响通知与用户日志，不影响 lifecycle listed 数据口径。
- 已确认：已监控用户在首次有库存时只收补货，不补发 listed。
- 已确认：保留额外详情通过新增 `catalog.listed.pending_stock` scope 实现，而不是扩展 Logs API 字段。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Rust 单元 / 集成测试覆盖：
  - 新 listed 且有库存。
  - 新 listed 零库存 -> pending_stock。
  - 同 lifecycle 下首次 `0 -> >0` -> listed 仅一次。
  - 已监控用户在首次 `0 -> >0` 只收补货。
  - relisted 后重新进入 pending -> listed。
- 日志查询测试确认 `catalog.listed.pending_stock` 可通过现有 `/api/logs` 返回。

### Quality checks

- `cargo fmt`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- `cd web && bun run lint && bun run typecheck`

## 文档更新（Docs to Update）

- `docs/specs/README.md`：新增本规格索引。
- 设置页 hint 文案：与规格保持一致，强调“首次有库存后通知”。

## 计划资产（Plan assets）

- Directory: `docs/specs/fswrs-listed-stock-gate/assets/`
- PR visual evidence source: 暂无；若后续需要 UI 证据，另行补充。

## Visual Evidence (PR)

## 资产晋升（Asset promotion）

None

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 冻结 follow-up spec、索引与分支命名
- [x] M2: 完成 DB / apply / lifecycle fanout 改造
- [x] M3: 完成设置文案与回归测试
- [ ] M4: 完成 fast-track 收口（push / PR / checks / review-loop / spec sync）

## 方案概述（Approach, high-level）

- 通过新增 `lifecycle_listed_event_at` 将“生命周期 active”与“已对外发出 listed”解耦：前者继续服务数据展示，后者专门负责通知幂等。
- apply 阶段在每次成功抓取后同时更新 lifecycle 状态与 listed-event 状态；fanout 只消费“本轮首次可发出的 listed ids”。
- 用户分流不新增设置，而是复用 `monitoring_configs.enabled` 判断谁该保留补货语义、谁该收到 listed。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：若未来补货语义也要去重到 lifecycle 级别，需要再定义 `restock` 与 `listed` 的优先级；本计划只处理首次有库存时的 listed 抑制。
- 风险：现有 rename / re-key 噪声仍会进入 pending_stock 或 listed；本计划不处理该识别精度问题。
- 需要决策的问题：None。
- 假设（需主人确认）：None。

## 变更记录（Change log）

- 2026-03-11: 初始化规格，冻结“listed 需首次有库存再发”的通知/日志分流范围与验收标准。
- 2026-03-11: 完成 DB 迁移、listed/pending_stock 分流、设置页文案与 Rust/Web 校验；待 fast-track PR/checks/review-loop 收口。

## 参考（References）

- `docs/specs/2vjvb-catalog-full-refresh-sse/SPEC.md`
- `docs/specs/cnduu-low-pressure-discovery-refresh/SPEC.md`
- `docs/specs/z9x5g-notification-copy-optimization/SPEC.md`
