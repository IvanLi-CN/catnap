# 懒猫云购物车库存监控（#0001）

## 状态

- Status: 已完成
- Created: 2026-01-18
- Last: 2026-01-20

## 背景 / 问题陈述

- 目标：对 `https://lazycats.vip/cart` 的产品“配置（plan/config）”做库存监控与通知，避免手动频繁刷新页面。
- 我们需要一个同仓 Web UI，提供四个模块：库存监控、全部产品、系统设置、日志。
- 需要在两个时点完成数据引导（bootstrap）：用户每次打开 Web UI、服务每次启动后，都要获取“国家地区 / 可用区域 / 配置”列表。
- 本系统仅提供同源访问的站点与接口，并且对外不暴露“用户权限识别/鉴权方式”的细节。

## 目标 / 非目标

### Goals

- 建立可持续的监控闭环：
  - 抓取并归一化：国家地区 / 可用区域 / 配置（规格、价格、库存数量）。
  - 支持“配置级”的监控开关（用户在 UI 里点选）。
  - 按固定频率轮询（默认 1 分钟），带抖动与失败可观测。
  - 库存数量 / 价格 / 配置变化触发通知，并在日志里可追溯。
- 冻结一组可实现、可测试的接口契约：
  - HTTP APIs（含错误形状、鉴权与同源约束）
  - 运行时配置（env）
  - 持久化 schema（默认 SQLite）

### Non-goals

- 不实现自动购买、自动下单、支付相关能力（仅监控与通知）。
- 不提供跨域访问能力（不提供 CORS；仅同源）。
- 不做多上游/多站点通用化（仅围绕 `lazycats.vip/cart`）。

## 用户与场景（Users & Scenarios）

- 用户：位于受信任网络/反向代理之后的内部用户。
- 场景：
  - 打开 Web UI，自动加载国家地区/区域/配置列表。
  - 在“全部产品”中以分组视图查看每个配置的规格、价格与库存，并切换是否监控。
  - 在“库存监控”中按国家地区与配置分组查看监控项（可折叠，默认展开）。
  - 在“系统设置”中配置：TG、浏览器推送、站点地址（默认取当前浏览器 URL）、查询频率（分钟，默认 1）+ 抖动。
  - 在“日志”中查看抓取/变化检测/通知的记录。

## 范围（Scope）

### In scope

- 上游抓取（lazycats cart）与数据引导：
  - 服务启动时抓取一次全量国家地区/区域/配置列表并缓存。
  - UI 打开时调用 bootstrap API 获取缓存数据，并在需要时触发刷新。
- Web UI（四模块）：
  - 库存监控：网格布局；一行一个可用区（region），行内为配置网格；支持折叠（默认展开）。
  - 全部产品：以合适的视图分组展示配置（规格/价格/库存），可一键切换监控。
  - 系统设置：TG、浏览器推送、站点地址、查询频率与抖动。
  - 日志：展示抓取/变化/通知相关日志（最小过滤/分页能力）。
- 轮询与变化检测：
  - 周期性抓取被监控配置对应的数据，记录最新状态。
  - 对库存数量 / 价格 / 配置变化进行检测与通知。
- 同源与鉴权：
  - API 仅同源访问（不提供 CORS；跨域请求不可用或返回 403）。
  - 通过受信任上游注入的用户信息识别用户；若请求缺少用户信息：
    - Web 访问返回 401 页面；
    - API 访问返回 JSON 401。
  - 任何错误信息/页面不得泄露用户识别细节。
- 持久化（默认 SQLite）：
  - 保存监控开关、系统设置、浏览器 push 订阅、关键日志与最近抓取结果（便于重启后恢复）。

### Out of scope

- 多租户/复杂权限体系（RBAC/ABAC）与组织管理（除非明确追加需求）。
- 大规模分布式监控平台与高可用架构（除非明确规模与 SLA）。

## 需求（Requirements）

### MUST

- 数据引导（bootstrap）：
  - 服务启动后主动抓取并缓存“国家地区 / 可用区域 / 配置”列表（失败要可观测）。
  - 用户每次打开 Web UI 必须拉取该列表与当前监控/设置状态（尽量单次请求完成）。
- 全部产品：
  - 以分组视图展示每个配置的规格信息、价格与库存数量。
  - 支持点击切换“是否监控库存”（状态持久化）。
- 库存监控：
  - 网格布局：一行一个可用区（region），行内为配置网格；支持折叠（默认展开）。
- 系统设置：
  - 通知参数：Telegram、浏览器推送、站点地址（默认读取当前浏览器 URL 作为默认值）。
  - 查询频率：`x` 分钟（默认 1），并带抖动。
- 通知触发：
  - 补货（库存数量从 0 变为 >0）
  - 价格变化
  - 配置变化（规格或可选项集合变化）
- 鉴权与同源：
  - 未获取到用户信息：Web 401 页面；API 返回 JSON 401（错误结构稳定）。
  - 站点接口仅同源访问（不提供 CORS；跨域请求不可用或返回 403）。
  - 不得暴露系统如何做用户权限识别（错误信息不得包含鉴权细节）。

### SHOULD

- 通知具备去重/节流避免刷屏（例如短时间内相同变化不重复推送）。
- 对抓取失败具备指数退避或短期重试策略（并写入日志）。
- 日志支持按时间/级别过滤与分页（最小能力即可）。

### COULD

- Playwright E2E 覆盖一条核心链路（切换监控 → 等待轮询 → 产生日志/通知）。
- 为不同通知类型提供开关（补货/价格/配置）。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Bootstrap | HTTP API | internal | New | ./contracts/http-apis.md | backend | web | 打开 Web UI 时加载国家地区/区域/配置/用户与设置 |
| Products | HTTP API | internal | New | ./contracts/http-apis.md | backend | web | 全部产品视图数据 |
| Monitoring | HTTP API | internal | New | ./contracts/http-apis.md | backend | web | 监控开关 + 监控页数据 |
| Settings | HTTP API | internal | New | ./contracts/http-apis.md | backend | web | 通知参数 + 查询频率（含抖动） |
| Logs | HTTP API | internal | New | ./contracts/http-apis.md | backend | web | UI 展示抓取/通知日志 |
| Web Push Subscription | HTTP API | internal | New | ./contracts/http-apis.md | backend | web | 浏览器推送订阅管理（如启用） |
| Runtime config | Config | internal | New | ./contracts/config.md | backend | ops | 环境变量与默认值（不向客户端暴露细节） |
| Persistence schema | DB | internal | New | ./contracts/db.md | backend | - | 默认 SQLite；用于设置、监控开关、订阅、日志 |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/http-apis.md](./contracts/http-apis.md)
- [contracts/config.md](./contracts/config.md)
- [contracts/db.md](./contracts/db.md)

## 验收标准（Acceptance Criteria）

- Given 用户打开 Web UI
  When 首次加载页面
  Then UI 自动请求 bootstrap，并渲染国家地区/区域/配置列表与默认分组视图。

- Given 用户在“全部产品”中切换某配置的监控开关
  When 刷新页面或服务重启
  Then 监控开关状态保持一致（持久化生效）。

- Given 系统按设置的频率轮询
  When 被监控配置库存数量 / 价格 / 配置发生变化
  Then 记录日志并触发通知（至少支持 TG；浏览器推送按决策实现）。

- Given 请求缺少受信任的用户信息
  When 访问 Web UI
  Then 返回 401 页面（不包含鉴权细节）。

- Given 请求缺少受信任的用户信息
  When 调用任意 `/api/*`
  Then 返回 JSON 401（错误结构稳定且不包含鉴权细节）。

- Given 非同源页面尝试访问本系统 API
  When 发起跨域请求
  Then API 不提供 CORS，并且不泄露额外信息（必要时返回 403）。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: 覆盖“上游页面解析与归一化”的关键规则与边界。
- Integration tests: 覆盖核心 API 的成功路径与失败路径（401/403/400）。
- E2E tests (if applicable): 覆盖一条端到端链路（可选）。

### Quality checks

- Backend: `cargo fmt`, `cargo clippy -D warnings`, `cargo test`.
- Frontend: `bun run lint`（Biome）, `bun run typecheck`, `bun run build`.

## 文档更新（Docs to Update）

- `README.md`: quick start（本地运行、Docker 运行、反向代理注入用户信息的操作说明、常用命令）。
- `docs/plan/0001:lazycats-cart-inventory-monitor/contracts/http-apis.md`: 随接口冻结与变更更新。
- `docs/plan/0001:lazycats-cart-inventory-monitor/contracts/config.md`: 运行时配置项与默认值。
- `docs/plan/0001:lazycats-cart-inventory-monitor/ui/README.md`: UI 信息架构与线框图（wireframes）。

## 里程碑（Milestones）

- [x] M1: 冻结“库存数量口径”与监控触发条件（补货/价格/配置变化）
- [x] M2: 冻结数据模型与持久化方案（默认 SQLite；按用户或全局）
- [x] M3: 冻结 HTTP API 契约（含错误形状、示例与同源约束）
- [x] M4: 冻结 UI 信息架构（四模块导航、分组视图与交互）
- [x] M5: 轮询与抖动策略冻结（默认 1 分钟 + 抖动 + 失败重试与节流）
- [x] M6: 通知渠道设计冻结（TG / Web Push / 站点地址链接）
- [x] M7: 进入实现阶段（impl）并按计划交付

## 开发计划清单（Checklist）

### Backend

- [x] 鉴权：读取 `CATNAP_AUTH_USER_HEADER` 指定的 header 获取用户标识；缺失时 Web 返回 401 页面、API 返回 JSON 401（不泄露鉴权细节）
- [x] 同源：API 不提供 CORS；对带 `Origin` 的请求做同源校验（失败返回 403）
- [ ] 抓取器（lazycats）：
  - [x] 抓取国家地区列表
  - [x] 抓取可用区域列表（含 fid/gid 映射）
  - [x] 抓取配置列表（pid/name/specs/price）
  - [x] 从 `/cart?fid=<fid>&gid=<gid>` 配置卡片解析 `库存： <n>`，并生成 `digest`（用于配置变化检测）；云服务器（`fid=2`）不解析库存字段
  - [ ] 抖动 + 失败重试 + 限流（避免打爆上游）
- [x] 持久化（SQLite）：按 `contracts/db.md` 建表与访问层，保存快照、监控开关、设置、订阅与日志
- [x] API：
  - [x] `GET /api/bootstrap`（一次返回 catalog + monitoring + settings + user）
  - [x] `GET /api/products`（全量/过滤）
  - [x] `GET /api/monitoring`（返回当前用户已监控配置）
  - [x] `PATCH /api/monitoring/configs/{configId}`（开关监控）
  - [x] `GET/PUT /api/settings`
  - [x] `GET /api/logs`（按用户隔离 + 分页）
  - [x] `POST /api/notifications/web-push/subscriptions`
- [x] 轮询与变化检测：
  - [x] 按“用户隔离”的设置计算各用户的有效轮询计划（同时尽量复用上游抓取结果，避免重复抓取）
  - [x] 对比差异：库存数量、价格、digest（配置变化）
  - [ ] 生成事件与去重/节流
- [x] 通知：
  - [x] Telegram：bot token + target（chat id/频道）可配置
  - [x] Web Push：VAPID + Service Worker + subscription
  - [ ] 通知内容包含：变化摘要、old/new、跳转链接（基于 `siteBaseUrl`）
- [x] 日志清理：按“天数 + 最大条数”双策略同时生效（默认 7 天 + 10000 条）

### Frontend (web/)

- [x] App 启动时调用 `GET /api/bootstrap` 获取初始化数据；失败/401 有明确 UI 状态
- [x] 导航：库存监控 / 全部产品 / 系统设置 / 日志
- [x] 全部产品：
  - [x] 分组视图（国家地区/区域/配置）
  - [x] 展示规格/价格/库存数量；一键切换“是否监控”
- [x] 库存监控：
  - [x] 一行一个可用区（region），行内为配置网格
  - [x] 支持折叠；默认展开；折叠状态本地记忆（可选）
- [x] 系统设置：
  - [x] 查询频率（分钟，默认 1）+ 抖动比例
  - [x] 站点地址默认值：`window.location.origin`（用户可改）
  - [x] Telegram 参数配置（不回显敏感字段）
  - [x] Web Push：申请权限、注册 service worker、上传 subscription
- [x] 日志：列表 + 过滤 + 分页

### Testing

- [x] 抓取解析：用保存的 HTML fixture 做单元测试（覆盖字段缺失与结构变化的容错）
- [x] API：集成测试覆盖 401/403/400 与核心成功路径
- [ ] E2E（可选）：切换监控 → 等待一次轮询 → 出现日志/通知

### Docs

- [x] `README.md`：本地运行、Docker 运行、配置项说明、反向代理注入用户 header 的示例（不暴露鉴权实现细节）

## Change log

- 2026-01-19: 落地后端 API + SQLite 持久化 + 上游抓取解析 + 轮询与 TG 通知；完善 Web UI（四模块 + Web Push 订阅上传）。
- 2026-01-19: UI 对齐线框图：顶部标题栏右侧控件按页面差异化（产品：刷新：手动；监控：最近刷新；设置/日志：无）；侧栏导航视觉降噪；“全部产品”补齐筛选与分组区；配置卡片改为“规格/价格/库存/更新/监控开关”布局；“库存监控”增加分组头与折叠按钮；“日志”页对齐过滤条/表格/分页 footer。
- 2026-01-19: UI 完全按 `ui/*.svg` 还原：恢复 `web/src/app.css` 并重构四页面布局/控件；更新 401 页面；Playwright（1440×900）复验截图：`tmp/ui-previews/playwright/impl-*.png`。
- 2026-01-19: 401 页面“返回首页”按钮 hover 光标为手形（`cursor: pointer`）。
- 2026-01-20: Web UI 增加后台自动刷新 + 路由切换刷新：按轮询间隔自适应（10–30s）拉取 `/api/products`，并在切到“库存监控/全部产品”时立即拉取一次，避免“更新时间”长期不变。
- 2026-01-20: 监控页右上角新增“重新同步”按钮：`POST /api/refresh` 启动同步、`GET /api/refresh/status` 查询进度；按钮展示“同步中（x/y）/同步完成”，失败显示感叹号并在头部下方弹出 DaisyUI 风格 Alert（30s 限流）。
- 2026-01-20: 细节修复：删除监控页内容区顶部“监控列表”区块；“重新同步”按钮放到顶栏“最近刷新”右侧；监控卡片“库存/更新”两枚 pill 固定宽度以消除多余空白；`fetch` 统一 `cache: no-store` + focus/visible 时主动刷新避免切页拿旧数据。
- 2026-01-20: 全部产品：移除配置卡片中“点击开关后立即持久化”提示（降低无意义信息噪声）。
- 2026-01-20: Web UI 自适应：整体居中（`max-width: 1440px`）并允许窗口缩小时不再左对齐；内容区与网格列宽支持收缩（避免横向溢出）。
- 2026-01-20: UI 细节：Topbar 增加上下内边距与 title/subtitle 间距；壳体高度改为跟随视口（避免底部出现大块空白）。

## 方案概述（Approach, high-level）

- 上游抓取策略（事实观察）：
- `lazycats.vip/cart` 页面包含“国家地区”“可用区域”“配置卡片”三层结构。
  - “国家地区”与“可用区域”的切换，会体现在 query 参数中：
    - `?fid=<fid>`：国家地区（示例：`/cart?fid=7`）
    - `?fid=<fid>&gid=<gid>`：国家地区 + 可用区域（示例：`/cart?fid=7&gid=6`）
  - `fid/gid` 的映射可从页面元素的 `onclick="window.location.href='/cart?...'"` 解析出来（示例：`onclick="window.location.href='/cart?fid=7'"`）。
  - 多数配置卡片会显示库存数量（例如 `库存： 4`），作为监控配置的“库存数量”来源。
  - 云服务器（`fid=2` / `country.name=云服务器`）在页面中不展示 `库存` 字段；该类配置**不需要监控**，并视为“长期有货/可购”（见 Decisions）。
  - 当页面展示 `库存： <n>` 时，`n=0` 通常意味着不可购（同页“立即购买”按钮会变为 `javascript:void(0)`）；未观察到“库存>0 但不可购”的反例。
  - **监控抓取仅访问 `/cart` 相关页面，不进入下单/结算流程页面**（例如不依赖 `action=configureproduct` 获取数据）。
- 缓存与刷新：
  - 服务启动 warm-up 一次全量抓取；后续以轮询刷新缓存。
  - UI 通过 bootstrap 获取缓存，并在必要时触发刷新（避免每次打开都全量打爆上游）。
- 同源与鉴权：
  - UI 由服务端同源托管；API 不提供 CORS，并在需要时对 `Origin` 进行同源校验。
  - 用户识别依赖受信任上游注入用户信息；对外错误信息一律不包含鉴权细节。

## UI 设计（Wireframes）

- `docs/plan/0001:lazycats-cart-inventory-monitor/ui/README.md`
- `docs/plan/0001:lazycats-cart-inventory-monitor/ui/inventory-monitor.svg`
- `docs/plan/0001:lazycats-cart-inventory-monitor/ui/products.svg`
- `docs/plan/0001:lazycats-cart-inventory-monitor/ui/settings.svg`
- `docs/plan/0001:lazycats-cart-inventory-monitor/ui/logs.svg`
- `docs/plan/0001:lazycats-cart-inventory-monitor/ui/unauthorized-401.svg`
- 临时预览（PNG，便于快速查看，可删）：
  - `docs/plan/0001:lazycats-cart-inventory-monitor/tmp/ui-previews/README.md`
  - `docs/plan/0001:lazycats-cart-inventory-monitor/tmp/ui-previews/*.png`

## 风险与开放问题（Risks & Open Questions）

- 风险：
  - 上游页面结构变更会导致解析失败（需要日志与回退策略）。
  - 上游可能存在反爬/限流策略（需要控制频率、抖动、缓存与失败重试策略）。
  - 上游“库存”展示文案/布局变更导致解析失败（需要 fixture 测试与快速回滚策略）。

### 已确认的决策（Decisions）

- 库存：以“数量”作为核心指标。
- 云服务器（`fid=2` / `country.name=云服务器`）：不支持监控（UI 不提供监控开关；API 拒绝开启），并视为“长期有货/可购”。对外返回 `inventory.status=available`，`inventory.quantity=1`（用于兼容统一数据结构；UI 展示为“有货”而非“1”）。
- 通知触发：补货、价格变化、配置变化。
- Telegram：可配置目标为“单个 chat id 或频道”。
- 浏览器推送：使用 Web Push（VAPID + Service Worker）。
- 监控与设置：按用户隔离。
- 日志保留默认值：`7` 天 + `10000` 条（“天数”和“条数”双策略同时生效，以更严格者为准；后续可通过 env 覆盖）。
- 监控页默认布局：一行一个可用区（region），行内为配置网格；支持折叠但默认展开。
- 抖动：接受默认 0–10%。

### 仍需主人确认（Open questions）

- None.

## 假设（Assumptions）

- 库存数量 `quantity` 为整数（>=0）。当页面展示 `库存： <n>` 时以其为准；云服务器（`fid=2`）不展示库存字段，统一返回“可购/有货”的占位值（见 Decisions）。
- 补货判定：`quantity` 从 `0` 变为 `>0`。
- 价格变化：`price.amount` 改变即触发（同币种、同周期）。
- 配置变化：规格列表或可选项集合发生变化即触发（以稳定 digest 对比）。
- 默认使用 SQLite 持久化；监控开关与通知配置按用户隔离。
- 默认查询频率 `1` 分钟，抖动比例 `0.1`（0–10% 随机延迟）。

## 参考（References）

- `docs/plan/README.md`
