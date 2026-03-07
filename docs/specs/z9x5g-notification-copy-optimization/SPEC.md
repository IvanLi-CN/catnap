# 通知文案优化：简洁告警风格（#z9x5g）

## 状态

- Status: 已完成
- Created: 2026-03-07
- Last: 2026-03-07

## 背景 / 问题陈述

- 现有通知正文直接暴露 `[config]`、`[listed]`、`lc:*` 等技术细节，普通用户很难一眼看懂“发生了什么”。
- 监控变化、上架/下架、测试消息分别在不同位置拼接文案，风格不统一，也不利于后续维护。
- 不做优化的代价是：通知可读性差、误报感强，用户需要打开日志或源代码才能理解提示含义。

## 目标 / 非目标

### Goals

- 将真实通知（监控变化、上架、下架）统一为“中文事件标题 + 核心状态 + 可选查看入口”的简洁告警风格。
- 将 Telegram / Web Push 测试消息改成用户可理解的默认文案，移除 `user=...`、原始 ISO 时间戳等技术噪音。
- 保留现有通知链路与 API 字段结构；内部日志继续维持机器可读风格，不被友好文案污染。

### Non-goals

- 不新增通知事件类型或独立开关。
- 不修改日志 scope、错误体系或 Telegram/Web Push 的 transport 行为。
- 不扩展真实监控变化到 Web Push（仍保持当前渠道覆盖范围）。

## 范围（Scope）

### In scope

- 共享通知文案 builder：统一事件标签映射、价格展示、链接拼接和测试消息默认值。
- `poller` 的监控变化 Telegram 通知改用友好文案。
- `ops` 的 listed/delisted Telegram 与 Web Push 文案改用友好文案。
- Telegram / Web Push 测试接口与前端触发逻辑改用后端默认文案。
- README 补充新的通知示例与字段取舍说明。

### Out of scope

- UI 内联状态提示（如“已发送。”）与错误提示的重写。
- 事件日志、ops 日志表、告警开关和鉴权模型的调整。
- 任何与 `docs/specs/` 无关的运行时资源迁移。

## 需求（Requirements）

### MUST

- 用户通知正文不再出现 `[restock]` / `[price]` / `[config]` / `[listed]` / `[delisted]` 和 raw `lc:*` 配置 ID。
- 价格文案与前端当前显示保持一致：CNY 显示 `¥x.xx / 月|年`。
- `siteBaseUrl` 存在时追加 `查看监控：<base>/monitoring`；不存在时整行省略。
- 日志仍保留现有机器可读消息格式，便于检索与排障。
- 默认 Telegram / Web Push 测试文案必须可读、稳定，不依赖前端硬编码。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 监控变化：
  - `config` -> `【配置更新】<name>` + `库存/价格`。
  - `restock` -> `【补货】<name>` + `库存 old → new｜价格`。
  - `price` -> `【价格变动】<name>` + `价格 old → new｜库存`。
  - 组合事件按固定顺序输出：`补货 + 价格变动 + 配置更新`。
- 生命周期通知：
  - `listed` -> `【新上架】<name>`；Web Push 标题为 `Catnap · 新上架`。
  - `delisted` -> `【已下架】<name>`；正文使用“最近状态”表述。
- 测试通知：
  - Telegram 默认文案：`【Telegram 测试】通知配置正常` + 说明 + 时间。
  - Web Push 默认文案：`Catnap · 测试通知` / `Web Push 已连通，点击返回设置页。`。

### 当前/优化后示例

- 监控配置变化
  - current: `[config] 芬兰特惠年付 Mini (lc:11:36:2f0b64cc00e0) qty=0 price=4.99`
  - proposed:
    ```text
    【配置更新】芬兰特惠年付 Mini
    库存 0｜¥4.99 / 年
    查看监控：https://<site>/monitoring
    ```
- 监控组合事件
  - current: `[restock,price] 芬兰特惠年付 Mini (lc:11:36:2f0b64cc00e0) qty=3 price=3.99`
  - proposed:
    ```text
    【补货 + 价格变动】芬兰特惠年付 Mini
    库存 0 → 3｜价格 ¥4.99 → ¥3.99 / 年
    查看监控：https://<site>/monitoring
    ```
- 新上架
  - current: `[listed] 芬兰特惠年付 Mini (lc:11:36:2f0b64cc00e0) qty=5 price=4.99 https://<site>/monitoring`
  - proposed (Telegram):
    ```text
    【新上架】芬兰特惠年付 Mini
    库存 5｜¥4.99 / 年
    查看监控：https://<site>/monitoring
    ```
  - proposed (Web Push): `Catnap · 新上架` / `芬兰特惠年付 Mini｜库存 5｜¥4.99 / 年`
- 测试消息
  - current Telegram: `catnap 测试消息\nuser=u_1\n2026-03-06T15:00:00Z`
  - proposed Telegram:
    ```text
    【Telegram 测试】通知配置正常
    如果你看到这条消息，说明 Catnap 已可发送 Telegram 通知。
    时间：2026-03-06 15:00:00Z
    ```
  - current Web Push: `title=catnap` / `body=测试通知 2026-03-06T15:00:00.000Z`
  - proposed Web Push: `title=Catnap · 测试通知` / `body=Web Push 已连通，点击返回设置页。`

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

None（`/api/notifications/telegram/test`、`/api/notifications/web-push/test` 的字段结构保持不变，仅调整默认文案来源与渲染规则）。

## 验收标准（Acceptance Criteria）

- Given `restock`、`price`、`config` 或其组合发生
  When 发送监控变化 Telegram 通知
  Then 用户看到中文事件标题与友好正文，不再看到 raw 事件标签与配置 ID。

- Given `listed` / `delisted` 触发
  When 发送 Telegram / Web Push 通知
  Then 标题与正文使用简洁告警风格，且 Web Push 的 title/body 与 Telegram 表达一致。

- Given 用户点击测试 Telegram / Web Push
  When 请求体未提供自定义文案
  Then 后端使用友好默认模板，而不是 `catnap` 或 `user=...` 技术文案。

- Given `siteBaseUrl` 为空
  When 构建用户通知
  Then 消息不出现空白链接行，正文仍自然可读。

## 实现前置条件（Definition of Ready / Preconditions）

- 已确认：只优化真实通知与测试消息；不扩展通知类型与开关。
- 已确认：日志与结构化 meta 继续保留机器可读格式。
- 已确认：组合事件顺序固定为 `补货 + 价格变动 + 配置更新`。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests：覆盖金额格式、监控变化文案、lifecycle 文案、默认测试文案。
- Integration tests：覆盖 Telegram 默认测试文案、Web Push 测试接口默认请求体、现有通知链路回归。

### Quality checks

- `cargo fmt`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- `cd web && bun run lint && bun run typecheck && bun run build`

## 文档更新（Docs to Update）

- `README.md`：补充新的 Telegram / Web Push 通知示例与字段取舍说明。
- `docs/specs/README.md`：新增索引行并同步状态。

## 计划资产（Plan assets）

- Directory: `docs/specs/z9x5g-notification-copy-optimization/assets/`
- PR visual evidence source: 暂无（本计划不要求截图）

## Visual Evidence (PR)

## 资产晋升（Asset promotion）

None

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 抽离共享通知文案 builder，统一金额/事件/链接规则
- [x] M2: 接入 poller、ops、Telegram/Web Push 测试通知链路
- [x] M3: 补充自动化测试并同步 README
- [x] M4: 完成 fast-track 收口（push / PR / checks / review-loop / spec sync）

## 方案概述（Approach, high-level）

- 新增独立文案构建层，复用现有 `Money` 语义与渠道发送函数，只替换“用户看到的文本”。
- 监控变化通知继续保留 `poll` 日志原文，用户通知改由 builder 输出友好文本。
- listed/delisted 继续保留现有 ops log scope，但 Telegram / Web Push 改为 channel-specific 文案（Telegram 多行，Web Push title/body/url）。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：若后续需要在通知中展示更多差异字段（例如具体 specs 变化），需要重新扩展 builder 数据输入。
- 需要决策的问题：None。
- 假设（需主人确认）：None。

## 变更记录（Change log）

- 2026-03-07: 初始化规格并冻结通知文案优化范围、示例与质量门槛。
- 2026-03-07: 完成 fast-track 收口，PR #59 已创建并进入 checks/review-loop 收敛。

## 参考（References）

- `docs/specs/35uke-billing-period-detection-fix/SPEC.md`
- `docs/specs/uqe6j-settings-notifications-test-button/SPEC.md`
