# 系统设置：通知测试按钮（#0008）

## 状态

- Status: 已完成
- Created: 2026-01-23
- Last: 2026-01-24

## 背景 / 问题陈述

- 当前「系统设置」支持配置 Telegram（bot token / target）与 Web Push（订阅上传），但缺少“可立即验证配置是否有效”的入口。
- 结果是：用户只能等到真实监控事件触发后才知道通知能否送达，排障成本高（尤其是 bot token / chat id 配错时）。
- 现状补充：Web Push 目前只有浏览器侧 Service Worker + subscription 上传（`POST /api/notifications/web-push/subscriptions`），后端未实现“发送 Web Push”链路（因此无法验证送达）。

## 目标 / 非目标

### Goals

- 在「系统设置 → 通知」里增加“测试”按钮：
  - Telegram：发送一条测试消息（临时发送，不自动保存输入的 token/target），并在 UI 里显示成功/失败原因。
  - Web Push：向当前浏览器发送一条测试 Push（并触发 Service Worker 展示通知），并在 UI 里显示成功/失败原因。
- 后端提供对应的内部 API，保证可实现、可测试，并遵守现有鉴权与同源限制。
- 补齐最小必要测试覆盖：至少覆盖参数校验与上游失败的错误呈现。

### Non-goals

- 不在本计划中实现“Web Push 与真实监控事件联动的推送”（即：库存变化时自动推送给所有订阅者）。本计划只要求“能发送测试 Push”。
- 不在本计划中改造通知模板/内容体系（仅提供固定的测试消息格式）。
- 不在本计划中引入新的鉴权机制或对外开放接口（保持 internal scope）。

## 范围（Scope）

### In scope

- UI：在 `web/src/App.tsx` 的 `SettingsViewPanel` 中，为 Telegram 增加“测试”按钮与状态提示（pending/success/error）。
- UI：为 Web Push 增加“测试”按钮与状态提示（pending/success/error）。
- API：新增 Telegram 测试通知 endpoint（见契约），支持“临时覆盖 token/target（不保存）”。
- API：新增 Web Push 测试 endpoint（见契约），向已保存的 subscription 发送测试 Push（不从请求体接收 endpoint；不保存新的 subscription）。
- Server：补齐“发送 Web Push”所需的最小实现（加密负载 + VAPID 签名 + 向 push service 发起请求），仅用于测试 endpoint。
- Observability：测试通知的成功/失败写入 `logs`（不包含 bot token 的任何明文）。
- Tests：后端新增集成测试覆盖 endpoint 的校验与失败路径（不访问真实 Telegram）。
- Docs：更新 `README.md` 的“通知配置 / Telegram”小节，补充“如何测试配置”。

### Out of scope

- Web Push 发送（需要额外的服务端密钥/发送实现与浏览器兼容性策略）。
- 增加新的配置项（除非为了测试可控性而必须；若需要将作为“需主人确认”的取舍）。

## 需求（Requirements）

### MUST

- Telegram 测试按钮在点击后（临时发送，不自动保存）：
  - UI 进入 pending 状态并禁用重复点击；
  - 成功时给出明确提示（例如“已发送”）；
  - 失败时展示可行动的错误信息（例如“缺少 bot token / target”“Telegram 返回 401/403”等）。

- Web Push 测试按钮在点击后：
  - UI 进入 pending 状态并禁用重复点击；
  - 若未授予通知权限或当前环境不支持 push，必须提示原因；
  - 成功时浏览器应展示一条通知（由 `web/public/sw.js` 处理）；
  - 失败时展示可行动的错误信息（例如“缺少 VAPID private key”“push service 返回非 2xx”等）。

- 后端测试 endpoint：
  - 必须复用现有的鉴权与 same-origin 防护（与其他 `/api/*` 一致）；
  - 必须对请求做最小校验（token/target 必填规则见契约）并在错误时返回 `400 INVALID_ARGUMENT`；
  - 上游请求失败时返回 `5xx` 并写入日志（不泄漏敏感信息）。
- 不在 API 响应与日志中回传/打印 bot token 明文。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `POST /api/notifications/telegram/test` | HTTP API | internal | New | ./contracts/http-apis.md | backend | Web UI | 发送测试消息 |
| `POST /api/notifications/web-push/test` | HTTP API | internal | New | ./contracts/http-apis.md | backend | Web UI | 发送测试 Push |
| VAPID keys（env vars） | Config | internal | Modify | ./contracts/config.md | backend | server | Web Push 发送所需 |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/http-apis.md](./contracts/http-apis.md)
- [contracts/config.md](./contracts/config.md)

## 验收标准（Acceptance Criteria）

- Given 用户在「系统设置」中输入可用的 Telegram `bot token` 与 `target`
  When 点击“测试 Telegram”
  Then UI 显示“已发送”（或等价成功提示），并且服务端不会在日志/响应中泄漏 `bot token`。

- Given Telegram 未配置完成（例如 `bot token` 与/或 `target` 缺失）
  When 点击“测试 Telegram”
  Then UI 显示可理解的失败原因；API 返回 `400`（`code=INVALID_ARGUMENT`）。

- Given Telegram 上游返回非 2xx（例如 token 无效导致 401/403）
  When 点击“测试 Telegram”
  Then UI 显示失败提示（包含 HTTP status 或可读等价信息）；服务端写入一条 `warn` 日志（不含 token 明文）。

- Given 当前环境支持 Service Worker + Push，且服务端已配置可用的 VAPID keys
  When 点击“测试 Web Push”
  Then 当前浏览器展示一条通知（标题/正文/跳转 URL 可识别为测试消息），并且 UI 显示成功提示。

- Given 服务端缺少 Web Push 发送所需配置（例如 VAPID private key）
  When 点击“测试 Web Push”
  Then UI 显示可理解的失败原因；API 返回 `5xx`（`code=INTERNAL`）。

## 实现前置条件（Definition of Ready / Preconditions）

- 目标/非目标与范围（in/out）已确认：测试覆盖 Telegram + Web Push。
- Telegram：允许使用已保存配置进行测试；也允许临时覆盖（不自动保存）。
- Web Push：环境变量口径已确认（需要补齐发送链路与 VAPID private/subject 配置）。
- 接口契约已定稿：见 `./contracts/`。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Integration tests（Rust / `tests/api.rs`）：
  - 参数校验：缺 token/target 时返回 `400 INVALID_ARGUMENT`
  - 上游失败：stub Telegram 返回非 2xx 时，API 返回 `5xx` 且记录 `warn` 日志
  - 成功路径：stub Telegram 返回 200 时，API 返回 `200`
  - Web Push：用本地 stub push service endpoint（`subscription.endpoint` 指向本地 server），验证 API 会向其发起请求并在 2xx 时返回 `200`

### UI / Storybook (if applicable)

- 更新 `web/src/stories/pages/SettingsViewPanel.stories.tsx`：
  - 展示 Telegram 区域新增的“测试”按钮与状态提示（至少确保布局与交互可见）。

### Quality checks

- 后端：`cargo fmt`、`cargo clippy -- -D warnings`、`cargo test`
- 前端：`bun run lint`、`bun run typecheck`、`bun run build`

## 文档更新（Docs to Update）

- `README.md`：Telegram 小节补充“测试按钮的用途与失败排查提示”（不记录任何敏感信息示例）。
- `README.md`：Web Push 小节补充“测试按钮的前置条件（VAPID keys/HTTPS）与失败排查提示”。

## 实现里程碑（Milestones）

- [x] M1: 后端新增 Telegram 测试 endpoint + 集成测试（stub Telegram）
- [x] M2: 后端补齐 Web Push 测试 endpoint（含 VAPID 配置）+ 集成测试（stub push service）
- [x] M3: 前端 `SettingsViewPanel` 增加 Telegram/Web Push 测试按钮与状态提示
- [x] M4: Storybook 与 `README.md` 同步更新

## 方案概述（Approach, high-level）

- UI 侧以“最小状态机”实现：idle → pending → success/error，并在 pending 时禁用按钮防重复点击。
- API 侧新增两个内部 endpoints：
  - Telegram：接收临时 token/target/text 并发送（不保存）；为可测试性，上游请求需要可被本地 stub（避免真实网络）。
  - Web Push：使用已保存 subscription + payload 并发送（不从请求体接收 endpoint）；发送所需 VAPID keys 由服务端配置提供。
- 记录日志用于排障：成功/失败都应写入 `logs`，但不包含敏感字段（bot token）。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：若测试实现直接调用真实 Telegram / 真实 push service，将导致测试不稳定或泄漏；必须通过本地 stub/注入方式避免。
- 假设：测试按钮不会自动“保存设置”；只在当前点击时做临时发送/测试（已确认）。

## 变更记录（Change log）

- 2026-01-23: 创建计划 #0008（待设计）。
- 2026-01-24: 完成实现：新增 2 个测试 endpoints；补齐 Web Push 发送链路（VAPID private/subject）；UI 增加测试按钮与状态提示；补齐集成测试与 README；Web Push endpoint 增加 SSRF 防护（不从请求体接收 endpoint）。
