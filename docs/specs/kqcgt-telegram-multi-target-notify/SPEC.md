# Telegram 多目标群/频道通知（#kqcgt）

## 状态

- Status: 部分完成（3/4）
- Created: 2026-03-14
- Last: 2026-03-14

## 背景 / 问题陈述

- 当前 Telegram 仅支持单个 `target`；一个 bot 无法同时向多个群、频道或私聊目标发送相同通知。
- 真实通知记录只有聚合 `telegram_status`，缺少逐目标投递结果，用户无法分辨“哪一个群/频道失败了”。
- 设置页的 Telegram 配置仍是单输入框，不适合维护多个目标，也无法一次性验证所有目标是否可达。

## 目标 / 非目标

### Goals

- 将 Telegram 设置从单个 `target` 升级为 `targets[]`，支持一个 bot 对多个目标顺序 fan-out。
- 设置页提供标签列表式目标编辑；测试按钮一次向全部目标发送测试消息并返回逐目标结果。
- 真实通知记录保存 Telegram 逐目标投递明细，并在 UI 中展示聚合状态与明细结果。
- 保持 Web Push 与其他通知链路现有行为不变。

### Non-goals

- 不支持每个 Telegram 目标使用独立 bot token。
- 不支持 Telegram 论坛 topic / thread id、分目标模板差异化、自动重试或并发 fan-out。
- 不新增新的通知渠道，也不改造 Web Push 契约。

## 范围（Scope）

### In scope

- 后端 settings / bootstrap / telegram test API / notification records API 的 Telegram 字段升级。
- SQLite schema 扩展：settings 多目标字段 + 通知投递明细表。
- `poller` 与 `ops` 的 Telegram 真实通知改为顺序 fan-out，并记录逐目标结果。
- Web UI 设置页的 Telegram 多目标编辑与测试反馈；通知记录页的 Telegram 逐目标结果展示。
- README、fixtures、stories、后端集成测试与前端类型/交互测试同步更新。

### Out of scope

- 新的 Telegram transport 抽象或队列/重试系统。
- 独立的 Telegram target 管理页、导入导出功能、搜索/筛选。
- 测试通知写入通知记录页。

## 需求（Requirements）

### MUST

- `SettingsView.notifications.telegram` 返回 `{ enabled, configured, targets[] }`，不再返回单 `target`。
- `SettingsUpdateRequest.notifications.telegram` 接收 `targets: string[] | null` 语义；后端按 trim、去空、保序去重规范化。
- 老数据若只有 `telegram_target`，读出时必须自动兼容成单元素 `targets[]`，不得丢历史配置。
- `POST /api/notifications/telegram/test` 必须支持一次测试全部目标，并在响应中返回每个目标的 `success/error` 结果。
- 真实通知记录必须保存每个 Telegram 目标的状态与错误信息；聚合状态新增 `partial_success`。
- 当 Telegram 启用但缺 bot token / targets 时，真实通知与测试通知都给出可操作错误，并保持敏感字段不泄漏。

### SHOULD

- 多目标 UI 保持输入顺序稳定，重复目标自动去重，删除/新增操作不打断自动保存体验。
- 通知记录页在聚合状态外，直接展示每个 Telegram 目标的最近结果与错误摘要。
- Ops 的 Telegram 成功率继续可观测，多目标场景按目标尝试记账，而不是按整条通知记 1 次。
- Telegram 本地配置缺失时，通知记录仍应展示可操作错误，但这类配置故障不计入按目标 fan-out 成功率。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- Settings 保存：
  - 用户在设置页编辑 Telegram targets 标签列表。
  - 前端自动保存时提交 `targets[]`；后端规范化后持久化到多目标字段，并将首个目标镜像到旧 `telegram_target`。
- Telegram 测试：
  - 点击“测试 Telegram”后，后端读取本次请求的 `targets[]`；若字段缺失或为 `null` 则回退到已保存配置，若显式提供但归一化后为空则直接返回缺少 targets。
  - 后端对每个目标顺序调用 Telegram Bot API，响应返回逐目标结果；前端显示整体成功/部分成功/失败与逐目标明细。
- 真实通知：
  - `poller` / `ops` 创建通知记录后，对全部 Telegram 目标顺序 fan-out。
  - 每个目标的成功/失败都写入投递明细表与日志；通知主记录的 `telegramStatus` 根据明细聚合。
  - 若 Telegram 已启用但缺少 bot token / targets，则记录一条本地配置错误明细用于 UI 诊断，但不把它算进按目标成功率统计。
- 通知记录展示：
  - 通知列表继续按通知组显示，但 Telegram 区域新增逐目标明细列表。
  - 单个目标失败时，组状态显示 `partial_success` 或 `error`，并能看到失败目标与错误信息。

### Edge cases / errors

- `targets[]` 全为空白或去重后为空时，视为缺少 Telegram target。
- 若部分目标成功、部分失败，则测试接口返回 200，并在 body 中标明整体 `partial_success`；真实通知记录同样聚合为 `partial_success`。
- 若全部目标失败，则测试接口返回 500，并附带逐目标错误；真实通知记录聚合为 `error`。
- 若 Telegram 未启用，则真实通知记录为 `skipped`；不会创建空的 Telegram 投递明细。
- 若旧客户端仍发送单 `target` 字段，不保证兼容；本次实现以当前仓库前后端同步升级为准。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Settings Telegram contract | HTTP API | internal | Modify | ./contracts/http-apis.md | backend | web | `target` -> `targets[]` |
| Telegram test API | HTTP API | internal | Modify | ./contracts/http-apis.md | backend | web | 返回逐目标结果 |
| Notification record deliveries | HTTP API | internal | Modify | ./contracts/http-apis.md | backend | web | `telegramDeliveries[]` + `partial_success` |
| Telegram multi-target storage | DB | internal | Modify | ./contracts/db.md | backend | backend | settings JSON + legacy mirror |
| Notification delivery details | DB | internal | New | ./contracts/db.md | backend | backend | 每目标一条投递结果 |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/http-apis.md](./contracts/http-apis.md)
- [contracts/db.md](./contracts/db.md)

## 验收标准（Acceptance Criteria）

- Given 老用户数据库里只有 `telegram_target`
  When 打开设置页或读取 `/api/bootstrap`
  Then Telegram 配置返回 `targets: [<legacy target>]`，且 `configured=true` 判定正确。

- Given 用户在设置页输入多个目标（含重复与空白）
  When 自动保存
  Then 后端按 trim、去空、保序去重存储，回读顺序稳定。

- Given Telegram 测试请求包含多个目标
  When 其中部分目标发送失败
  Then API 返回逐目标结果与整体 `partial_success`，前端可展示成功/失败目标。

- Given 真实通知发送到多个 Telegram 目标
  When 其中一个目标失败
  Then 通知记录页显示聚合 `partial_success`，并能看到失败目标及错误信息；ops notify 统计按目标尝试累积。

- Given Telegram 未配置完成
  When 用户点击测试或真实通知触发 fan-out
  Then 系统返回/记录“缺少 bot token 或 targets”之类的可操作错误，且不泄漏 token 明文。

## 实现前置条件（Definition of Ready / Preconditions）

- 多目标发送范围已明确：共享单 bot token、顺序 fan-out、无重试。
- 公共接口升级口径已冻结：settings/test/notification records 统一使用 `targets[]` 与 `telegramDeliveries[]`。
- 通知记录聚合状态新增 `partial_success` 已确认。
- 兼容策略已确认：旧 `telegram_target` 仅用于读取兼容与首目标镜像。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Rust integration tests：settings 兼容回读、多目标测试 API、真实通知逐目标 fan-out、通知记录明细读写。
- Frontend tests / stories：设置页多目标编辑、测试按钮逐目标反馈、通知记录逐目标明细展示。

### UI / Storybook (if applicable)

- 更新 `web/src/stories/pages/SettingsViewPanel.stories.tsx`。
- 如有必要，补充通知记录页 fixtures / stories。

### Quality checks

- `cargo fmt`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- `cd web && bun run lint`
- `cd web && bun run typecheck`
- `cd web && bun run build`

## 文档更新（Docs to Update）

- `README.md`：Telegram 配置与测试方式改为多目标。
- `docs/specs/README.md`：新增本规格索引；实现推进后同步状态。

## 计划资产（Plan assets）

- Directory: `docs/specs/kqcgt-telegram-multi-target-notify/assets/`
- In-plan references: `![...](./assets/<file>.png)`
- PR visual evidence source: maintain `## Visual Evidence (PR)` in this spec when PR screenshots are needed.

## Visual Evidence (PR)

- None yet.

## 资产晋升（Asset promotion）

None

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 冻结多目标 Telegram settings/test/notification-record 契约与 schema 兼容策略
- [x] M2: 完成后端多目标存储、fan-out 发送与通知投递明细持久化
- [x] M3: 完成前端多目标设置 UI、测试反馈与通知记录明细展示
- [ ] M4: 完成 README / stories / tests / 质量门，并收敛 PR + review-loop

## 方案概述（Approach, high-level）

- 在不拆散现有 Telegram transport 的前提下，引入“target 规范化 + 顺序 fan-out + 聚合状态计算”三层能力。
- 用 settings JSON 字段承载多目标主存储，避免为单用户设置再建子表；同时为通知记录单独建投递明细表，保证历史记录可追踪。
- 前端以轻量标签编辑器承载多目标输入，不引入新路由或复杂表单库。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：多目标 fan-out 会放大单次通知耗时，但当前目标数量预期较小，顺序发送可接受。
- 风险：若逐目标错误直接长文本展示，通知记录 UI 可能变拥挤；实现时需压缩为短摘要。
- 需要决策的问题：None。
- 假设（需主人确认）：None。

## 变更记录（Change log）

- 2026-03-14: 创建规格，冻结 Telegram 多目标设置、测试与通知记录明细契约。
- 2026-03-14: 完成多目标 settings/test/notification-record 实现，质量门已通过，待 PR 与 review-loop 收口。
- 2026-03-14: review-loop 修正删除后立即测试、显式空 targets 语义、投递顺序稳定性与配置错误诊断边界。

## 参考（References）

- `docs/specs/uqe6j-settings-notifications-test-button/SPEC.md`
- `docs/specs/xm4p2-notification-records-telegram-deeplink/SPEC.md`
- `docs/specs/z9x5g-notification-copy-optimization/SPEC.md`
