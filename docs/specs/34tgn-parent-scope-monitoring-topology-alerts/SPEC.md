# 父级监控子级变更与拓扑告警（#34tgn）

## 状态

- Status: 已完成
- Created: 2026-03-11
- Last: 2026-03-11

## 背景 / 问题陈述

- 当前三级监控语义混杂：可用区监控只覆盖套餐上新，全站开关仍会广播任意分区的套餐上架，而地区层并不存在独立的拓扑监控入口。
- 现有删除提示是单独的全局开关，无法表达“谁在关心这个父级对象，就由谁接收对应子级删除”的范围规则。
- products 页只按 `countryId + regionId?` 扁平分组，缺少地区标题层，用户无法在界面上直观看到“地区监控”和“可用区监控”的层级差异。
- topology probe/refresh 已能发现地区与可用区增删，但当前只记日志，不会投递“新地区 / 新可用区 / 对象删除”提示。

## 目标 / 非目标

### Goals

- 把监控语义固定为三级“父级监控子级变更”：
  - 全站开关监控地区新增/删除。
  - 地区监控监控可用区新增/删除。
  - 可用区监控监控套餐新增/删除。
- products 页重排为“地区标题块 -> 可用区块 -> 套餐卡片”，并提供地区监控与可用区监控两个层级的按钮。
- 设置页收口为 3 个监控开关：套餐变更、可用区变更、地区变更；删除类提示并入各层级开关，不再保留独立删除开关。
- 新地区/新可用区提示需附带当前抓到的套餐摘要；当对象刚出现但暂无套餐时，也要显式告知“当前未发现套餐”。
- 复用 `monitoring_partitions(countryId, regionId?)`，其中 `regionId = null` 明确表示“地区监控”。

### Non-goals

- 不改变卡片级单配置监控的库存 / 价格 / 配置变更链路。
- 不新增独立的地区/可用区监控页面，也不扩展 monitoring 页承载范围。
- 不改动 Telegram / Web Push transport 配置方式，不引入新的通知通道。
- 不新增全站监控实体表；全站层仍由 settings 全局开关表达。

## 范围（Scope）

### In scope

- `SettingsView.monitoringEvents` / `SettingsUpdateRequest.monitoringEvents` 改为：
  - `partitionCatalogChangeEnabled`
  - `regionPartitionChangeEnabled`
  - `siteRegionChangeEnabled`
- 历史字段迁移：
  - `partitionListedEnabled -> partitionCatalogChangeEnabled`
  - `siteListedEnabled -> siteRegionChangeEnabled`
  - `regionPartitionChangeEnabled` 默认为 `false`
  - 旧 `delistedEnabled` 退出运行时读写与响应结构。
- `products` 页新增地区标题层，并在地区标题和可用区标题上分别展示对应监控开关。
- 套餐新增/删除通知继续走 catalog apply diff；地区/可用区新增/删除通知改走 topology diff。
- 通知收件人按父级监控范围路由，并禁止跨层重复投递同一事件。
- 新地区/新可用区通知正文列出最多 10 条套餐摘要（名称 + 价格），超出时补充总数说明。

### Out of scope

- 不改变 `/api/monitoring/configs/:configId` 的接口语义。
- 不引入“地区监控联动启用其下全部可用区监控”之类批量状态写入。
- 不扩展 `/api/monitoring` 返回地区/可用区监控摘要。

## 需求（Requirements）

### MUST

- `monitoring_partitions(countryId, regionId)` 仅表示可用区监控；`monitoring_partitions(countryId, null)` 仅表示地区监控，不再兼容旧的“无可用区国家分区监控”混合语义。
- 全站监控只能发送地区新增/删除；不得继续发送任意套餐上架/下架广播。
- 地区监控只能发送该地区下可用区新增/删除；不得直接为该地区下的套餐新增/删除发消息。
- 可用区监控只能发送该可用区下套餐新增/删除；不得升格为可用区新增/删除或地区新增/删除。
- 新地区 / 新可用区通知必须包含该对象当前已发现的套餐摘要；若没有套餐，正文必须明确“当前未发现套餐”。
- 可用区/地区删除只允许在正式 `topology_refresh` 路径中判定，避免 probe 抖动导致误删提示。
- topology diff 必须尊重 ambiguous-country preserve 逻辑，避免把“保留旧可用区”误报成新增或删除。

### SHOULD

- 同一用户对同一事件只收到一条通知；跨层命中时以事件所属层级为准，不做额外升级或降级。
- products 页“仅看已监控”应把地区监控、可用区监控、卡片级监控都算作命中条件。
- 设置页提示文案要明确说明各开关只负责哪一层子级变更，避免把“新增/删除”与“卡片轮询变更”混淆。

### COULD

- 通知 meta 可记录 `scopeKind = site_region | region_partition | partition_catalog`，方便后续日志筛选。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 用户在 products 页打开某个地区的“地区监控”：
  - 前端调用 `/api/monitoring/partitions`，写入 `{ countryId, regionId: null, enabled }`。
  - 后端将该记录视为“地区监控 scope”，后续仅对这个地区下的可用区新增/删除生效。
- 用户在 products 页打开某个可用区的“可用区监控”：
  - 仍写入 `{ countryId, regionId, enabled }`。
  - 后续仅对这个可用区下的套餐新增/删除生效。
- 用户在 settings 页开启“地区变更”：
  - 后续任意新地区 / 地区删除事件都会通知该用户。
- topology probe / refresh 发现新地区或新可用区：
  - 立刻构造一条对象级通知，并附上当前该对象下的套餐摘要（最多 10 条）。
  - 若套餐为空，则正文写“当前未发现套餐”。
- topology refresh 发现地区或可用区删除：
  - 只向监控该父级的用户发送删除通知。
  - 删除通知不携带旧套餐清单，只报告被删除对象名称与层级。
- catalog apply 发现套餐新增或删除：
  - 只向监控该可用区且开启“套餐变更”的用户发送。
  - 同一套餐事件不再走全站广播。

### Edge cases / errors

- 对已有显式可用区的地区，`regionId = null` 监控记录仍合法，但语义固定为地区监控，而不是“默认可用区”。
- 新地区首次出现但还没抓到任何可用区/套餐时，仍视为地区新增并发送空摘要提示。
- 新可用区首次出现但该页面尚无套餐时，仍视为可用区新增并发送空摘要提示。
- 若 topology probe 结果在 merge 后只是在更新现有地区/可用区名称或说明，不应触发新增提示。
- 若 topology refresh 因 ambiguous-country preserve 保留了旧可用区，不应触发可用区删除提示。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Bootstrap monitoring payload | HTTP API | internal | Modify | ./contracts/http-apis.md | backend | web | `enabledPartitions` 复用，`monitoringEvents` 字段改名 |
| Monitoring partition toggle | HTTP API | internal | Reuse | ./contracts/http-apis.md | backend | web | `regionId = null` 明确表示地区监控 |
| Settings monitoring events | HTTP API | internal | Modify | ./contracts/http-apis.md | backend | web | 3 个层级变更开关 |
| Parent-scope monitoring persistence | DB | internal | Reuse | ./contracts/db.md | backend | backend | `monitoring_partitions` 同时承载地区/可用区 scope |
| Topology change notification routing | Internal service | internal | New | ./contracts/db.md | backend | backend | topology diff -> recipients |

### 契约文档（按 Kind 拆分）

- [contracts/http-apis.md](./contracts/http-apis.md)
- [contracts/db.md](./contracts/db.md)

## 验收标准（Acceptance Criteria）

- Given 用户在 products 页打开某个地区监控
  When 该地区后续新增或删除可用区
  Then 用户收到对应的可用区新增/删除通知；该地区下套餐新增/删除不会因地区监控而直接通知。

- Given 用户在 products 页打开某个可用区监控
  When 该可用区后续出现套餐新增或删除
  Then 用户收到套餐新增/删除通知；同一事件不会额外触发全站广播。

- Given 用户在 settings 页开启 `siteRegionChangeEnabled`
  When topology 发现新地区或地区删除
  Then 用户收到地区级通知，且新增地区通知正文附带该地区当前套餐摘要或“当前未发现套餐”。

- Given topology probe 发现新可用区但该可用区暂无套餐
  When 通知发送
  Then 正文必须明确“当前未发现套餐”，且仍标识这是新可用区事件。

- Given 历史数据库中 `partitionListedEnabled = 1`、`siteListedEnabled = 1`
  When 新版本启动并读取 `/api/settings`
  Then 响应中 `partitionCatalogChangeEnabled = true`、`siteRegionChangeEnabled = true`、`regionPartitionChangeEnabled = false`，且不再返回 `delistedEnabled` / `siteListedEnabled`。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Backend：覆盖 settings 迁移、新旧字段兼容读取、地区/可用区/套餐三级收件人路由、topology add/remove diff、ambiguous preserve 不误报。
- Frontend：覆盖 products 的地区标题/可用区标题布局、地区/可用区监控按钮回写、settings 三开关持久化与 Storybook fixtures 更新。

### Quality checks

- `cargo fmt`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- `cd web && bun run lint`
- `cd web && bun run typecheck`
- `cd web && bun run build`
- `cd web && bun run test:storybook`

## 里程碑（Milestones）

- [x] M1: 冻结 follow-up spec 与接口契约，明确父级监控子级变更的语义边界。
- [x] M2: 完成后端 settings 迁移、parent-scope 收件人路由、topology diff 通知与测试。
- [x] M3: 完成前端 products/settings 交互重构、Storybook 校验与快车道交付收敛。

## 方案概述（Approach, high-level）

- 保留 `monitoring_partitions` 单表，但将 `regionId = null` 语义收敛为地区监控；可用区与地区通过 `scopeKind` 推导而非新增表。
- 将“套餐新增/删除”和“拓扑新增/删除”拆成两套通知构建器：前者继续按单套餐发送，后者按地区/可用区对象发送。
- topology 新增使用 merge 后的 snapshot diff，删除使用正式 refresh 前后 diff，并在 diff 前应用 ambiguous-country preserve。
- 前端通过二级分组结构把地区标题独立出来，使地区监控按钮与可用区监控按钮在视觉上属于不同层级。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：`regionId = null` 的旧记录若曾被用户当作“默认可用区”理解，升级后会自动转义为“地区监控”，需要 UI 文案足够清晰。
- 风险：topology probe 与 refresh 并行时，新增/删除 diff 需要严格限定触发源，避免重复提示或先删后加的抖动。
- 风险：新地区/新可用区通知列套餐摘要时，若套餐很多，需严格裁剪到 10 条并保证文案长度可控。
- 假设：监控页继续只展示卡片级单配置监控，不承载地区/可用区/全站监控摘要。
- 假设：全站地区删除提示无需附带历史套餐列表，只需说明被删除地区。

## 变更记录（Change log）

- 2026-03-11: 创建 follow-up spec，冻结三级“父级监控子级变更”语义与 topology 告警方向。
- 2026-03-11: 完成三级监控链路、topology 新增/删除通知、products/settings 重构，并通过 cargo/bun/Storybook 本地质量门。

## 参考（References）

- `docs/specs/32dfj-partition-monitoring-new-machine-alerts/SPEC.md`
- `docs/specs/cnduu-low-pressure-discovery-refresh/SPEC.md`
- `docs/specs/z9x5g-notification-copy-optimization/SPEC.md`
