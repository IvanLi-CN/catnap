# 机器资产：面板与 VNC 新窗口按钮（#j3zhd）

## 状态

- Status: 已完成
- Created: 2026-03-21
- Last: 2026-03-22

## 背景 / 问题陈述

当前 `#machines` 页面需要在机器卡片层同时支持两类入口：一类是直接打开缓存中的 Web 面板
`panel_url`，另一类是进入真实网页 VNC 控制台。缓存里的 `panel_url` 只代表容器面板入口，
不等于真实的控制台链接；若把它直接当成 VNC 打开，最终会落到 dashboard 而不是
`/console?token=...`。

## 目标 / 非目标

### Goals

- 在机器列表卡片侧边保留“打开 VNC”按钮，并以新窗口/新上下文打开真实网页控制台。
- 在机器列表卡片侧边新增“打开面板”按钮，并以新窗口直接打开缓存的 Web 面板入口。
- 每次点击按钮时，由服务端实时解析该机器的最新网页 VNC 链接，而不是依赖旧缓存。
- 仅对具备对应入口的机器启用按钮；不具备时保持禁用态。

### Non-goals

- 不实现内嵌 noVNC 页面、VNC 代理、鉴权透传或任何写操作。
- 不改动懒猫账号同步 cadence、面板缓存策略或流量历史链路。
- 不把容器 dashboard 页面继续伪装成“VNC 已就绪”的最终入口。

## 范围（Scope）

### In scope

- `POST /api/lazycat/machines/:service_id/vnc-url`：点击时实时解析网页 VNC 链接。
- 前端机器卡片新增“打开面板”按钮，直接基于 `panelUrl` 新窗口打开 Web 面板。
- 服务端优先尝试 live token 链路，失败时回退到解析面板 HTML 中的真实 console 链接。
- 前端机器卡片按钮改为点击时请求该 API，再用返回的 URL 新窗口打开。
- Storybook 与 API 测试覆盖“实时返回 console URL”“打开 Web 面板”及“入口缺失禁用”场景。

### Out of scope

- 新增面板端凭据配置或替换现有懒猫抓取方式。
- 为非容器/NAT 机器生成任何伪 VNC 入口。

## 验收标准（Acceptance Criteria）

- Given 某台机器具备容器面板能力，且服务端能够实时拿到最新的网页控制台 token
  When 用户点击“打开 VNC”
  Then 前端必须以新窗口打开该次点击实时返回的 `/console?token=...` URL。

- Given 某台机器存在缓存的 `panel_url`
  When 用户点击“打开面板”
  Then 前端必须以新窗口直接打开该 `panel_url`，且不得走 VNC token 解析链路。

- Given live token 链路暂时不可用，但面板 HTML 中已包含真实 console 链接
  When 用户点击“打开 VNC”
  Then 服务端仍必须解析并返回该真实 console URL，而不是直接回退到 dashboard。

- Given 某台机器没有容器面板能力
  When 用户查看机器列表
  Then 卡片仍显示“打开 VNC”按钮，但按钮必须处于禁用态，且不会触发请求与跳转。

- Given 某台机器没有缓存的 Web 面板入口
  When 用户查看机器列表
  Then 卡片仍显示“打开面板”按钮，但按钮必须处于禁用态，且不会触发跳转。

- Given 服务端本次点击无法解析出真实网页 VNC 控制台入口
  When 用户点击“打开 VNC”
  Then 前端必须展示错误信息，且不得打开缓存 dashboard 作为错误兜底。

## 非功能性验收 / 质量门槛（Quality Gates）

- Rust: `cargo test --all-features`
- Web: `bun run lint` + `bun run typecheck` + `bun run build` + `bun run test:storybook`
- 不引入机器卡片桌面端/移动端布局回退。

## 实现里程碑（Milestones）

- [x] M1: 明确“真实网页 VNC 入口必须在点击时动态解析”的边界
- [x] M2: 服务端新增实时解析 API，并优先走 live token 链路
- [x] M3: 前端按钮切换为点击时请求 + 新窗口打开实时结果
- [x] M4: 前端补充“打开面板”按钮并与 VNC 按钮职责拆分
- [x] M5: 测试与 Storybook 覆盖实时 console URL / Web 面板行为

## 风险 / 假设

- 假设：容器主机名可由当前机器缓存字段 `service_code` 稳定提供。
- 假设：live token 接口不可用时，面板 HTML 中仍可能暴露真实 console 链接，可作为回退解析来源。
- 风险：若未来上游同时关闭 token 接口与 HTML console 链接暴露，则需要新的显式 VNC API 合同。
