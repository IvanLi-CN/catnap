# 分区级监控与双上新通知（#32dfj）

## 状态

- Status: 已完成
- Created: 2026-03-10
- Last: 2026-03-10

## 背景 / 问题陈述

- 当前 products 页只支持卡片级监控，范围仅覆盖补货 / 价格 / 配置变化；用户无法直接对“德国 / 德国特惠”这类分区整体订阅上新。
- 现有“上架监控”是单一全局开关，语义混合了“只看我关心的分区”与“全站任意分区都通知”两种诉求。
- 不拆分的话，用户要么被全站新机噪音打扰，要么错过未逐张配置开启监控但仍关心的分区上新。

## 目标 / 非目标

### Goals

- 让 products 页现有分组（`countryId + regionId?`）可单独开启“分区上新机”监控。
- 把原有单一“上架监控”拆成“分区上新机 / 全站上新机 / 下架监控”三个设置项。
- 保持卡片级监控语义不变；分区开关不批量改写配置卡片监控状态。
- 让 listed/relisted 生命周期通知支持“分区优先、全站兜底”的去重投递。

### Non-goals

- 不新增新的业务层级或独立的拓扑模型；继续复用现有 products 分组粒度。
- 不改变 delisted 通知范围、recentListed24h 页面展示口径，或卡片级补货 / 价格 / 配置变更链路。
- 不引入按分区过滤的 recentListed24h 视图，也不批量联动现有配置卡片监控开关。

## 范围（Scope）

### In scope

- `BootstrapResponse.monitoring` 增加 `enabledPartitions`。
- 新增分区监控 toggle API，用 `countryId + regionId?` 持久化用户分区订阅状态。
- `SettingsView` / `SettingsUpdateRequest` 的 `monitoringEvents` 调整为 `partitionListedEnabled / siteListedEnabled / delistedEnabled`。
- 数据迁移：旧 `monitoring_events_listed_enabled` 自动映射到新的 `siteListedEnabled`。
- listed 生命周期通知根据订阅类型区分“分区上新机 / 全站上新机”，且同一事件优先走分区通知避免重复。
- products 页分组头部新增“分区上新”开关；settings 页替换三项通知设置文案。

### Out of scope

- 不改动卡片级 `/api/monitoring/configs/:configId` 接口语义。
- 不改动 Telegram / Web Push transport 配置方式。
- 不为分区监控增加独立日志页面、独立最近事件列表或新的权限模型。

## 需求（Requirements）

### MUST

- 分区定义必须与 products 页分组完全一致：`countryId + regionId?`。
- 分区开关状态必须持久化，并通过 bootstrap 返回给前端。
- 历史用户若旧上架监控为开启，升级后必须表现为 `siteListedEnabled=true`、`partitionListedEnabled=false`。
- listed/relisted 事件若同时命中分区与全站通知，同一用户默认只收到一条，且优先使用“分区上新机”。
- 分区级监控只影响 listed/relisted 通知，不得改写任意配置卡片的 `monitorEnabled`。

### SHOULD

- 分区通知文案应能区分“分区上新机”和“全站上新机”。
- 分区 toggle 接口应拒绝空 `countryId`，并仅接受当前 catalog 中可识别的 country/region 组合。
- stale 分区订阅即使暂时不出现在当前页面，也应保留在用户订阅数据中，待分组再次出现时自动恢复匹配。

### COULD

- 在分组头部提示“只影响分区上新机，不影响卡片监控”以减少误解。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 用户在 products 页分组头部打开“分区上新”：
  - 前端调用分区 toggle API。
  - 后端写入用户对该 `countryId + regionId?` 的分区订阅。
  - 刷新页面后 bootstrap 返回该分区在 `enabledPartitions` 中，分组头部继续显示开启。
- 用户在 settings 页开启“分区上新机”：
  - 仅当 listed/relisted 事件落在该用户已订阅分区时发送通知。
- 用户在 settings 页开启“全站上新机”：
  - listed/relisted 事件对所有分区都生效，包括未订阅分区与未在 products 中手动开启分区监控的分区。
- 同一用户同时开启“分区上新机”和“全站上新机”，且事件命中已订阅分区：
  - 只发送一条“分区上新机”通知，不重复追加“全站上新机”。
- delisted 通知：
  - 仍只受 `delistedEnabled` 控制，范围与当前版本保持一致。

### Edge cases / errors

- 若请求的 `countryId` 为空，或 `regionId` 与该 `countryId` 不匹配，分区 toggle API 返回 `400 INVALID_ARGUMENT`。
- country-only 分区使用 `regionId=null`；不得把空字符串与 `null` 当成不同分区。
- 历史老库升级后，若新字段不存在，启动时完成 schema 补齐与 listed 开关回填，不要求手工迁移脚本。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Bootstrap monitoring payload | HTTP API | internal | Modify | ./contracts/http-apis.md | backend | web | `monitoring.enabledPartitions` |
| Monitoring partition toggle | HTTP API | internal | New | ./contracts/http-apis.md | backend | web | 分区订阅写接口 |
| Settings monitoring events | HTTP API | internal | Modify | ./contracts/http-apis.md | backend | web | 三段式 listed/delisted 设置 |
| Monitoring partition persistence | DB | internal | New | ./contracts/db.md | backend | backend | 用户分区订阅表 |
| Settings listed migration | DB | internal | Modify | ./contracts/db.md | backend | backend | 旧 listed -> siteListed |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/http-apis.md](./contracts/http-apis.md)
- [contracts/db.md](./contracts/db.md)

## 验收标准（Acceptance Criteria）

- Given 用户在 products 页某个分组头部打开分区监控
  When 刷新页面或服务重启
  Then bootstrap 的 `monitoring.enabledPartitions` 包含该分区，且该分区下卡片的 `monitorEnabled` 状态与操作前完全一致。

- Given 用户开启 `partitionListedEnabled` 且订阅“德国 / 德国特惠”
  When 该分区出现 listed 或 relisted
  Then 用户收到一条“分区上新机”通知；其他未订阅分区的 listed 不触发该通知。

- Given 用户开启 `siteListedEnabled`
  When 任意分区出现 listed 或 relisted
  Then 用户收到一条“全站上新机”通知，包括未订阅分区与未开启分区监控的分区。

- Given 同一 listed 事件同时命中分区订阅与全站开关
  When 通知发送
  Then 同一用户只收到一条通知，且类型为“分区上新机”。

- Given 历史数据库中 `monitoring_events_listed_enabled = 1`
  When 新版本启动并读取 `/api/settings`
  Then 响应中 `monitoringEvents.siteListedEnabled=true`、`monitoringEvents.partitionListedEnabled=false`，`delistedEnabled` 原值保留。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Backend：覆盖 bootstrap/settings shape、分区 toggle 持久化、旧 listed 开关迁移、listed 通知路由去重。
- Frontend：覆盖 products 分组头部分区 toggle、本地状态回写、settings 三个通知开关持久化与 story fixtures 更新。

### Quality checks

- `cargo fmt`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- `cd web && bun run lint`
- `cd web && bun run typecheck`
- `cd web && bun run build`
- `cd web && bun run test:storybook`

## 里程碑（Milestones）

- [x] M1: 冻结 API / DB 契约并完成规格落盘
- [x] M2: 完成后端分区订阅持久化、settings 迁移与 listed 通知路由改造
- [x] M3: 完成前端 products/settings 交互、测试验证与 PR 收敛

## 方案概述（Approach, high-level）

- 后端用新的用户分区订阅表保存 `countryId + regionId?` 开关，并在 bootstrap 中返回启用分区集合。
- settings 保留旧列以兼容老库，但实现逻辑切换到新 `partition/site listed` 字段；迁移通过启动期 schema backfill 自动完成。
- listed 生命周期通知在 ops 层按“分区订阅用户集合”和“全站开关用户集合”分流，再用用户 ID 去重，优先发送分区通知。
- Web 端只在分组头部体现分区开关，并显式提示“不会影响卡片监控”，避免语义混淆。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：旧 listed 列与新 site listed 列长期共存，若迁移条件处理不当，可能反复覆盖用户最新设置。
- 风险：listed 通知按分区和全站分流后，需要避免同一 run 对同一用户重复发送。
- 需要决策的问题：None（本规格采用“分区优先、全站兜底”去重策略）。
- 假设（需主人确认）：None。

## 变更记录（Change log）

- 2026-03-10: 创建规格，冻结分区订阅、双 listed 通知与迁移口径。
- 2026-03-10: 完成后端分区订阅持久化、通知路由改造、前端 products/settings 交互与全量本地质量门。
- 2026-03-10: 创建 PR #63，并完成 checks 收敛与 review-loop 复核（无阻塞项）。

## 参考（References）

- `docs/specs/2vjvb-catalog-full-refresh-sse/SPEC.md`
- `docs/specs/cnduu-low-pressure-discovery-refresh/SPEC.md`
- `docs/specs/z9x5g-notification-copy-optimization/SPEC.md`
