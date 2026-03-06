# 设置页通知测试成功气泡化（#vycru）

## 状态

- Status: 部分完成（3/4）
- Created: 2026-03-06
- Last: 2026-03-06

## 背景 / 问题陈述

- 既有「系统设置 → 通知」中的 Telegram / Web Push 测试成功反馈使用行内灰字提示，位置与错误气泡不一致，视觉优先级偏低。
- 在 `/Users/ivan/.codex/worktrees/3990/catnap/docs/specs/uqe6j-settings-notifications-test-button/SPEC.md` 中，测试按钮能力已完成；本次仅跟进成功反馈呈现，不扩展旧 spec 的接口与后端范围。

## 目标 / 非目标

### Goals

- 将 Telegram 与 Web Push 测试成功提示统一改为按钮左侧锚定气泡。
- 复用现有错误气泡布局，新增 success tone，并补齐自动消失与手动关闭。
- 补齐 Storybook 成功态场景、自动消失验收，以及 PR 视觉证据。

### Non-goals

- 不修改 Telegram / Web Push 测试 API 的请求响应契约。
- 不修改保存成功提示（如“已自动保存”）或非测试场景的成功反馈。
- 不变更失败提示来源、错误文案策略与红色错误气泡交互。

## 范围（Scope）

### In scope

- `web/src/App.tsx`：抽象通用反馈气泡组件，并将 Telegram / Web Push 测试成功态改为气泡。
- `web/src/app.css`：补充 success bubble 视觉 token、tone 样式与响应式适配。
- `web/src/stories/pages/SettingsViewPanel.stories.tsx`：新增成功态故事与交互验收。
- `docs/specs/vycru-settings-test-success-bubbles/assets/`：存放 Storybook 视觉证据。

### Out of scope

- 后端 endpoint、日志与鉴权逻辑。
- 设置页其他成功态（自动保存、启用 Push 成功等）。
- 真实程序截图或桌面级截图。

## 验收标准（Acceptance Criteria）

- Given Telegram 测试接口返回成功
  When 点击“测试 Telegram”
  Then 按钮进入 pending 且禁点，成功后按钮左侧显示 success bubble，不再显示行内 `已发送。`。

- Given Web Push 测试接口返回成功
  When 点击“测试 Web Push”
  Then 按钮进入 pending 且禁点，成功后按钮左侧显示 success bubble，不再显示行内灰字成功说明。

- Given success bubble 已显示
  When 用户手动关闭或等待 4 秒
  Then 气泡消失；再次发起测试时旧气泡不会叠加。

- Given 任一测试接口失败
  When 请求完成
  Then 不显示 success bubble，只保留现有错误气泡与错误消息来源。

- Given light / dark 主题与移动端断点
  When 渲染 success bubble
  Then 定位、箭头与关闭按钮行为与错误气泡一致，且不发生明显裁切。

## 实现里程碑（Milestones）

- [x] M1: 通用反馈气泡组件支持 `error` / `success` tone 与可访问性语义
- [x] M2: Telegram / Web Push 成功态切换为气泡，并加入 4 秒自动消失
- [x] M3: Storybook 成功场景与交互断言通过
- [ ] M4: 视觉证据入库并用于 PR

## Visual Evidence (PR)

- 待补充

## 变更记录（Change log）

- 2026-03-06: 创建 follow-up spec，范围锁定为设置页通知测试成功反馈气泡化。
- 2026-03-06: 完成前端 success bubble 与 Storybook 交互验收；待补充 PR 视觉证据。
