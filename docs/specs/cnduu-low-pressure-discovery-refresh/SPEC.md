# 低压优先的上架发现优化（#cnduu）

## 状态

- Status: 已完成
- Created: 2026-03-08
- Last: 2026-03-08

## 背景 / 问题陈述

- 当前“上架发现”主要依赖两条链路：已启用监控配置的用户轮询，以及按小时级触发的全量刷新。未监控页面上的新上架往往要等到下次全量刷新才会被发现。
- 现有启动 warm-up 会直接走全量抓取，且热路径中还会补扫 `configureproduct` 页面恢复 `pid`，这会在服务重启或冷启动时对上游造成明显的瞬时压力。
- 生命周期通知目前只在手动全量刷新链路中触发，导致“DB 已经知道新上架、UI 也可能已展示，但未及时通知”的割裂体验。

## 目标 / 非目标

### Goals

- 在“全局轻扫 + 监控快扫”的前提下，将未监控页面的新上架发现收敛到 `3–5 分钟`。
- 将上游抓取改为低压优先：`DB-first` 启动、拓扑低频复扫、已知 URL 轻量 discovery、请求级限流与 freshness 复用。
- 统一生命周期事件触发口径：`poller_due`、`discovery_due`、`manual_refresh` 任一成功抓取后均执行 listed/delisted/relisted 判定与通知。
- 保持现有 HTTP API 非 breaking，仅补充必要观测字段与文案语义修正。

### Non-goals

- 不新增终端用户可配置的 discovery 频率开关。
- 不依赖上游 `304 / ETag / Last-Modified` 做条件请求优化。
- 不将产品下单、`pid` 补全、或 `configureproduct` 页面探测纳入 discovery/monitoring 的时延关键路径。

## 范围（Scope）

### In scope

- 启动链路：`DB-first` catalog 启动、空库拓扑初始化、禁用启动阶段 `configureproduct` 热路径补扫。
- 调度链路：新增 `topology_refresh`、`discovery_due`，保留 `poller_due` 与 `manual_refresh`，并为不同 reason 定义固定 freshness window。
- 上游抓取协调器：host 级限流（单并发 + 冷却）、recent success 复用、统一 queue reason 与 cache hit 语义。
- 生命周期事件：任意成功抓取后统一 diff 与通知，且保证同一状态迁移只通知一次。
- Web UI：监控页 `recentListed24h` 后台刷新；设置页不再暴露拓扑复扫开关，仅展示系统固定的拓扑复扫间隔。
- Ops 观测：展示 discovery/cache-hit、最近一次 topology refresh、队列最老等待时长。

### Out of scope

- 新增多站点、多租户或跨站通用抓取框架。
- 为 discovery 增加新的外部 API。
- 在本计划中实现新的图片、视觉证据或 Storybook 资产。

## 需求（Requirements）

### MUST

- 服务启动在本地 DB 已有 catalog 数据时不得立即发起全站 warm-up；应直接以 DB 数据服务读请求。
- 本地 DB 为空或 catalog 拓扑缺失时，启动阶段仅允许执行 `root + fid` 拓扑初始化；`gid/url_key` 页面抓取必须交给后续 `discovery_due` 渐进完成。
- 上游请求预算固定为：
  - 任意时刻最多 `1` 个 in-flight upstream request；
  - 任一 upstream request 完成后至少冷却 `500ms` 才能发出下一个真实请求。
- freshness window 固定为：
  - `poller_due = 45s`
  - `discovery_due = 150s`
  - `manual_refresh = 300s`
  - `auto/topology_refresh = 1800s`
- 若同一 `url_key` 在对应 freshness window 内已有成功抓取，则后续任务必须走 cache hit，不再请求上游，但仍需正常推进队列、日志与状态展示。
- 当多个 reason 合并到同一任务时，freshness window 以其中最宽的窗口为准，避免因队列去重反向放大上游请求。
- `configureproduct` / `pid` 探测不得出现在启动、discovery、monitoring 热路径；发现链路中的相关请求数必须为 `0`。
- `poller_due`、`discovery_due`、`manual_refresh` 任一成功抓取后，均需执行生命周期差异计算与通知分发；listed/delisted/relisted 的去重语义必须与当前状态机一致。
- 若 `manual_refresh` 的强制 real-fetch 请求在同一 `url_key` 任务已进入 cache-hit 判定后才合并进来，协调器仍必须补做真实抓取，不能让 late joiner 只拿到旧 cache-hit 结果。
- 监控页 `recentListed24h` 必须在 DB 中出现新上架后 `<=30s` 内可见，无需等待手动全量刷新结束。
- `settings.catalogRefresh.autoIntervalHours` 字段保留，但改为只读的系统固定值 `12`（小时），不再作为终端用户可编辑设置。

### SHOULD

- `discovery_due` 应覆盖所有已知 `url_key`，并采用分摊式渐进扫描，避免单轮扫完整站。
- 空库启动后，已知 `gid/url_key` 应在 `<=5 分钟` 内完成首轮 discovery 建档。
- Ops 仪表盘应能明确区分 `fetch` 与 `cache hit`，并按 reason 聚合 `discovery_due`、`poller_due`、`manual_refresh`、`topology_refresh`。

### COULD

- 为后续低预算离线 `pid` 补全预留 reason 或后台任务命名，但本计划不要求对终端用户暴露。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 启动（非空 DB）
  - 服务启动时读取 DB 中已有 catalog snapshot 与已知 `url_key`。
  - 若存在可用 catalog，则 API 直接返回 DB 数据，不触发全量上游 warm-up。
  - 若 catalog 配置已存在但拓扑表缺失，则后台需立即补一次 `root + fid` 拓扑初始化，避免升级后长时间返回空 countries/regions。
  - 后台仅按计划启动 `topology_refresh` / `discovery_due` / `poller_due` 调度。
- 启动（空库）
  - 服务启动后请求 root 页面，枚举 `fid`。
  - 对每个 `fid` 请求国家页，建立国家/可用区拓扑与 `url_key` 列表。
  - 不在此阶段请求全部 `gid` 页面；改由 `discovery_due` 在后续 5 分钟窗口内逐步完成。
- discovery_due
  - 调度器按固定 cadence 选择“最久未成功抓取”的已知 `url_key` 入队。
  - 命中 freshness window 时返回 cache hit；未命中才发起真实抓取。
  - 成功抓取后统一 apply diff、更新 last success、刷新 lifecycle 状态与 `recentListed24h` 查询结果。
- poller_due
  - 仍按用户监控配置驱动，但调用同一抓取协调器。
  - 若目标 `url_key` 在 45 秒窗口内已有成功抓取，则直接复用结果，不重复打站。
- manual_refresh / topology_refresh
  - `manual_refresh` 仍以“推进所有已知 URL 子任务”为语义，默认优先 cache hit；仅在 region notice 初始化缺失时允许对单个 `url_key` 触发真实抓取补齐状态。
  - `topology_refresh` 负责低频更新 root/fid/gid 拓扑，并刷新已知 `url_key` 集合；若某个 `fid` 页面既解析不到 region 也解析不到 config，则必须保留该国家既有 region 拓扑，不能直接 retire 旧 target。其 UI 文案统一解释为“目录拓扑复扫”。
- UI 可见性
  - `products` 继续按现有 10–30 秒后台刷新策略拉取 `/api/products`。
  - `monitoring` 在现有进入页面刷新之外，增加 10–30 秒后台刷新 `/api/monitoring`，确保 `recentListed24h` 能及时反映 discovery 结果。

### Edge cases / errors

- 上游请求失败或解析失败时，不得覆盖最近成功快照，也不得产生 delisted 判定。
- 若单个 `url_key` 因限流或 freshness 命中未发起真实请求，queue/ops 中仍需记录 reason 与 cache hit 结果。
- 若 discovery 与 poller 同时命中同一 `url_key`，必须去重并复用同一任务结果。
- 若配置经历 `delisted -> active`，视为 relisted，仍计入 `recentListed24h` 且仅通知一次。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| SettingsView catalog refresh copy | HTTP API | internal | Modify | ./contracts/http-apis.md | backend | web | 字段名不变，语义改为“目录拓扑复扫” |
| Monitoring polling behavior | HTTP API | internal | Modify | ./contracts/http-apis.md | backend | web | 保持响应形状不变，仅约束刷新时机 |
| Ops queue/state payload | HTTP API | internal | Modify | ./contracts/http-apis.md | backend | web | 补充 discovery/cache-hit/老化观测 |
| Catalog topology metadata | DB | internal | Modify | ./contracts/db.md | backend | backend | 支撑 DB-first 启动与 discovery 扫描 |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/http-apis.md](./contracts/http-apis.md)
- [contracts/db.md](./contracts/db.md)

## 验收标准（Acceptance Criteria）

- Given 本地 DB 中已有 catalog 数据
  When 服务重启
  Then 服务启动后不得立即对上游执行全站 warm-up，且启动阶段 `configureproduct` 探测请求数为 `0`。

- Given 本地 DB 为空
  When 服务首次启动
  Then 首轮只允许请求 root 页面与各 `fid` 页面，不得同步扫完整个 `gid` 页面集合；所有已知 `url_key` 必须在后续 `<=5 分钟` 的 discovery 周期内逐步建档。

- Given 同一 `url_key` 在 `150s` 内已被成功抓取
  When `discovery_due` 或 `manual_refresh` 再次命中该 `url_key`
  Then 系统必须返回 cache hit，不得发出新的 upstream request。

- Given 同一 `url_key` 在 `45s` 内已被成功抓取
  When `poller_due` 再次命中该 `url_key`
  Then 系统必须复用已有结果，不得重复请求上游。

- Given discovery 或 poller 成功抓取后发现新增配置
  When apply diff 完成
  Then DB 中该配置必须被标记为 listed/active，且若用户启用了 listed 通知，必须发出与 manual refresh 同等级别的通知。

- Given monitoring 页面已打开
  When DB 中 `recentListed24h` 集合因 discovery 更新而变化
  Then 页面必须在 `<=30s` 内显示最新列表，无需等待手动全量刷新。

- Given owner 查看 ops 面板
  When 任意 `discovery_due` / `poller_due` / `manual_refresh` / `topology_refresh` 任务执行
  Then 面板必须能区分 fetch 与 cache hit，并展示最近一次 topology refresh 与队列最老等待时长。

## 实现前置条件（Definition of Ready / Preconditions）

- 低压优先的请求预算（单并发 + `500ms` 冷却）已冻结。
- freshness window 已冻结为 `45s / 150s / 300s / 1800s`。
- `autoIntervalHours` 的新语义已确认：表示系统固定的目录拓扑复扫间隔 `12h`，而不是强制全量刷新周期。
- 不新增新的终端用户 discovery 配置项。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests:
  - 调度选择与 freshness window 命中/未命中判定
  - lifecycle diff 在 `discovery_due` / `poller_due` / `manual_refresh` 下的一致性
- Integration tests:
  - 非空 DB 启动不触发全站 warm-up
  - 空库启动仅抓 root/fid，后续 discovery 渐进补全 `url_key`
  - ops/state 返回 discovery/cache-hit/队列老化指标
- E2E tests (if applicable):
  - monitoring 页面后台刷新后展示新的 `recentListed24h`

### UI / Storybook (if applicable)

- Stories to add/update:
  - Monitoring 页面：`recentListed24h` 后台刷新后的状态
  - Settings 页面：展示系统固定的 `12h` 目录拓扑复扫间隔（只读）
  - Ops 页面：discovery/cache-hit 指标展示

### Quality checks

- Rust: `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features`
- Web: `cd web && bun run lint`, `cd web && bun run typecheck`, `cd web && bun run test:storybook`

## 文档更新（Docs to Update）

- `README.md`: 更新 `CATNAP_OPS_WORKER_CONCURRENCY`、全量刷新/自动刷新文案说明，澄清 topology refresh 与 discovery 的角色。
- `docs/specs/README.md`: 增加本计划索引并在交付完成后更新状态。
- `docs/specs/2vjvb-catalog-full-refresh-sse/SPEC.md`: 在实现完成后补充“被本计划约束覆盖”的关联说明（如需要）。

## 计划资产（Plan assets）

- Directory: `docs/specs/cnduu-low-pressure-discovery-refresh/assets/`
- In-plan references: `![...](./assets/<file>.png)`
- PR visual evidence source: maintain `## Visual Evidence (PR)` in this spec when PR screenshots are needed.
- If an asset must be used in impl (runtime/test/official docs), list it in `资产晋升（Asset promotion）` and promote it to a stable project path during implementation.

## Visual Evidence (PR)

## 资产晋升（Asset promotion）

None

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 后端实现 `DB-first` 启动与 topology refresh / discovery_due 三层调度，并冻结 freshness window 与 host 级限流。
- [x] M2: 后端统一 lifecycle diff/notify 触发点，移除 discovery/monitoring 热路径中的 `configureproduct` 探测。
- [x] M3: API / ops / Web UI 完成低压优先可观测性与 `recentListed24h` 后台刷新，测试补齐并通过。

## 方案概述（Approach, high-level）

- 以“已知 `url_key` 的轻量 discovery”替代“频繁全量刷新”，将上游请求从 burst 型改为均匀分摊型。
- 启动阶段优先利用 DB 中已知 catalog 与 URL 拓扑；若拓扑数据缺失，仅补最小必要的 root/fid 初始化，不恢复全站 warm-up。
- 所有抓取入口统一接入同一协调器，保证限流、去重、cache hit、lifecycle diff、通知与 ops 观测口径一致。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：
  - 若 root/fid 页面结构变化，空库启动可能无法快速恢复完整拓扑；需要保持失败可观测且不污染已知快照。
  - 单并发 + 冷却可能在站点极慢时拉长首轮 discovery 建档时间；需通过“最老等待时长”观测来判断是否需要后续调整。
- 需要决策的问题：
  - None
- 假设（需主人确认）：
  - 上游站点仍会继续暴露稳定的 root/fid/gid 页面层级，且当前 3–5 分钟 discovery SLA 可在单并发预算下达成。

## 变更记录（Change log）

- 2026-03-08: 创建低压优先上架发现优化规格，冻结 SLA、请求预算与交付里程碑。
- 2026-03-08: 完成实现与验证：DB-first 启动、topology persistence、discovery/cache-hit/queue aging 观测，以及 monitoring `recentListed24h` 后台刷新。

## 参考（References）

- `docs/specs/2vjvb-catalog-full-refresh-sse/SPEC.md`
- `docs/specs/ynjyv-ops-collection-dashboard/SPEC.md`
