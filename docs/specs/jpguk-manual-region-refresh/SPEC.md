# 手动刷新单个可用区（#jpguk）

## 状态

- Status: 已完成
- Created: 2026-03-15
- Last: 2026-03-15

## 背景 / 问题陈述

- 当前 Catnap 只能触发整站目录刷新；当主人只想确认某个可用区是否刚补货时，必须等待全量刷新跑完，范围过大、反馈也不够聚焦。
- `products` 页已经按国家 / 可用区展示 scope，用户心智天然落在“这个可用区现在重新抓一次”。
- 如果继续只提供全量刷新，短周期人工核实单个可用区时会反复消耗整站刷新预算，也会把全局刷新状态与局部确认动作混在一起。

## 目标 / 非目标

### Goals

- 为 `products` 页的具名可用区提供独立“手动刷新”入口。
- 点击后仅刷新对应 `(countryId, regionId)` scope，并强制真实抓取上游，不复用短期 cache hit。
- 刷新完成后静默回拉 bootstrap/products，让配置、空态与可用区说明一起同步。
- 保持现有全局目录刷新、SSE 进度与 monitoring 页面语义不变。

### Non-goals

- 不新增国家级手动刷新入口。
- 不给 monitoring 页增加同款刷新按钮。
- 不改变 `manual_refresh` 以外的 ops reason 口径。
- 不引入新的全局 refresh manager 或新的 SSE 事件流。

## 范围（Scope）

### In scope

- 新增单可用区刷新 HTTP API。
- 后端复用 `OpsManager` 的单目标排队/去重/force-fetch 能力。
- `products` 页可用区标题区新增局部刷新按钮与 scoped 状态反馈。
- 前端成功后静默刷新 `GET /api/bootstrap` 与 `GET /api/products`。
- 覆盖 topology-only 可用区、非法 scope、局部失败等测试。

### Out of scope

- 国家标题层的局部刷新。
- `monitoring` 页 UI 变更。
- `/api/catalog/refresh`、`/api/catalog/refresh/events` 的语义调整。
- 新的配置项、轮询策略或通知策略。

## 需求（Requirements）

### MUST

- `POST /api/catalog/refresh/partition` 仅接受现存具名可用区 scope；`regionId` 不能为空。
- 点击可用区刷新后必须执行真实抓取，即使该 scope 的 URL cache 仍在 5 分钟 freshness window 内。
- 刷新完成后，页面上该可用区相关的套餐、空态、notice 与监控开关上下文必须保持一致，不需要用户手动整页刷新。
- 局部刷新错误只能影响该可用区的反馈，不得污染全局目录刷新状态。
- 若同目标任务已在队列中，局部刷新必须安全等待并把任务升级为 force-fetch，而不是生成冲突抓取。

### SHOULD

- 局部刷新按钮应在 topology-only 可用区仍可用，方便用户在空 scope 上主动拉一次最新状态。
- 刷新中的视觉反馈应清楚表达“仅这个可用区正在刷新”。

### COULD

- 成功反馈可短暂显示“已刷新”态，再恢复默认图标。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 用户在 `products` 页看到某个具名可用区块，标题操作区新增“刷新”图标按钮。
- 用户点击后，前端向 `POST /api/catalog/refresh/partition` 发送 `{ countryId, regionId }`。
- 后端先校验该 scope 仍存在于 catalog topology；通过后直接调用 `enqueue_and_wait_force_fetch(countryId, Some(regionId), "manual_refresh")`。
- 请求成功返回后，前端顺序静默刷新 `bootstrap` 与 `products`，让 topology、notice、configs 与 filter 结果同步到最新状态。
- 局部按钮在请求期间进入 loading/disabled，仅该 scope 受影响；成功后显示短暂 success，失败后显示 scoped error。

### Edge cases / errors

- 若 `regionId` 缺失、为空、或对应 scope 已从 topology 移除，API 返回 400。
- 若后端抓取失败，API 返回 500；前端只在对应可用区显示失败反馈，不改动全局 refresh pill/SSE。
- 若当前 scope 只有 topology、没有任何套餐，按钮仍可点击；刷新后若仍无套餐，保留空态但更新时间与 notice 允许更新。
- 若同一个 scope 已在 cache-hit 流程中排队，局部刷新必须把该任务升级为 force-fetch，并等待最新真实抓取结果。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Partition catalog refresh | HTTP API | internal | New | ./contracts/http-apis.md | backend | web | 仅刷新具名可用区 |

### 契约文档（按 Kind 拆分）

- [contracts/http-apis.md](./contracts/http-apis.md)

## 验收标准（Acceptance Criteria）

- Given `products` 页渲染了具名可用区块
  When 页面加载完成
  Then 该可用区标题操作区可见独立刷新按钮，国家标题与 monitoring 分组没有该入口。

- Given 某个可用区 URL cache 仍在 freshness window 内
  When 用户点击该可用区刷新
  Then 后端仍执行真实抓取，而不是直接返回 cache hit。

- Given 某个可用区目前只有 topology、暂无套餐
  When 用户点击刷新且上游仍无套餐
  Then 该可用区块继续保留空态，但 notice / fetched data 可更新，页面不丢失该 scope。

- Given 请求中的 `(countryId, regionId)` 不存在或 `regionId` 为空
  When 调用 `POST /api/catalog/refresh/partition`
  Then 返回 400，且不会创建新的 ops 任务。

- Given 同一个 `(countryId, regionId)` 已经有在途 cache-hit 任务
  When 用户再次点击该可用区刷新
  Then 现有任务被安全升级为 force-fetch，最终结果来自真实抓取，且不会重复创建冲突任务。

## 实现前置条件（Definition of Ready / Preconditions）

- 刷新入口仅限 `products` 页具名可用区 scope 已冻结。
- 局部刷新固定使用真实抓取而非 cache reuse 已冻结。
- HTTP 请求 / 响应 shape 与错误口径已冻结。
- 相关测试范围（后端接口 + 前端交互/Storybook）已明确。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit / integration tests: 新增后端 API 与 ops 合流测试。
- UI tests: 新增 Storybook/Vitest 覆盖局部刷新按钮状态、topology-only 空态与失败反馈。

### UI / Storybook (if applicable)

- Stories to add/update: `ProductsView.stories.tsx` 增加具名可用区局部刷新的交互场景。
- Visual regression baseline changes (if any): 无强制新增视觉证据；若 PR 需要截图，可补入本 spec 的 `assets/`。

### Quality checks

- Rust: `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, 相关 `cargo test`。
- Web: `bun run lint`, `bun run typecheck`, 相关 story tests。

## 文档更新（Docs to Update）

- `docs/specs/README.md`: 登记该 spec。
- `docs/specs/jpguk-manual-region-refresh/contracts/http-apis.md`: 冻结接口契约。
- `README.md`: 若最终需要对外说明“支持单可用区手动刷新”，实现后再补充。

## 计划资产（Plan assets）

- Directory: `docs/specs/jpguk-manual-region-refresh/assets/`
- In-plan references: `![...](./assets/<file>.png)`
- PR visual evidence source: maintain `## Visual Evidence (PR)` in this spec when PR screenshots are needed.

## Visual Evidence (PR)

## 资产晋升（Asset promotion）

None

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新增单可用区刷新 API，并接入现有 ops force-fetch 调度。
- [x] M2: `products` 页可用区标题新增局部刷新按钮、局部状态反馈与静默数据回拉。
- [x] M3: 补齐后端与前端测试，覆盖非法 scope、topology-only scope 与 force-fetch 合流。
- [x] M4: 完成质量门禁、PR、checks 与 review-loop 收敛。

## 方案概述（Approach, high-level）

- 后端直接复用当前 `OpsManager` 的单目标队列能力，避免再造一套 mini refresh manager。
- 前端采用按 partition key 管理的局部异步状态，不把单 scope 刷新接入全局 catalog refresh 条。
- 数据同步继续走既有 `bootstrap/products` 读取路径，避免新增局部 bootstrap endpoint。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：局部刷新成功后如果只回拉 `products` 而不回拉 `bootstrap`，会漏掉 topology/notice 更新；本计划要求两者都刷新。
- 风险：局部按钮如果复用全局 refresh 状态，会让用户误解为整站刷新；本计划明确隔离状态。
- 假设：`countryId` 与上游 `fid` 一致，`regionId` 与上游 `gid` 一致，可直接作为 ops target 使用。

## 参考（References）

- `docs/specs/cnduu-low-pressure-discovery-refresh/SPEC.md`
- `docs/specs/2vjvb-catalog-full-refresh-sse/SPEC.md`
- `docs/specs/34tgn-parent-scope-monitoring-topology-alerts/SPEC.md`
