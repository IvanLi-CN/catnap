# 下架产品归档与三态恢复显示（#4ccac）

## 状态

- Status: 已完成
- Created: 2026-03-03
- Last: 2026-03-03

## 背景 / 问题陈述

当前配置在下架后会持续出现在产品相关页面，缺少“用户侧清理归档”能力，导致噪音堆积。产品重新上架时也没有与归档状态联动的自动恢复逻辑。

## 目标 / 非目标

### Goals

- 增加按用户独立的下架归档模型，记录清理时间 `cleanupAt`。
- 提供手动“一次性归档全部下架项”能力，默认隐藏已归档下架项。
- 在“全部产品”提供三态筛选：`仅正常 / 全部 / 仅归档`；监控页固定按“仅正常”语义展示且不提供归档筛选控件。
- 当配置重新上架时，自动清空归档状态并恢复默认可见。

### Non-goals

- 不做自动归档（检测到下架即自动写入）。
- 不做单条归档或勾选批量归档。
- 不做筛选偏好持久化（如 localStorage / 服务端设置）。

## 范围（Scope）

### In scope

- DB 新增 `user_config_archives(user_id, config_id, cleaned_at)`。
- API 新增 `POST /api/products/archive/delisted`。
- `Config.lifecycle` 扩展 `cleanupAt`。
- 产品页新增归档确认弹窗（卡片预览）和一键归档入口。
- 产品页三态筛选；监控页默认隐藏归档下架项且不显示归档筛选控件。
- relist 自动清理归档记录。

### Out of scope

- 调整监控策略、通知策略、刷新策略。
- 引入新的全局消息中心或通知系统。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `user_config_archives` | SQLite table | internal | Add | `contracts/db.md` | backend | backend | 用户独立归档记录 |
| `POST /api/products/archive/delisted` | HTTP API | internal | Add | `contracts/http-apis.md` | backend | web | 幂等批量归档 |
| `Config.lifecycle.cleanupAt` | HTTP JSON field | internal | Modify | `contracts/http-apis.md` | backend | web | 可选字段，未归档时不返回 |

## 验收标准（Acceptance Criteria）

- Given 存在“下架且未归档”配置
  When 用户在“全部产品”点击“一键归档下架”并确认
  Then 返回归档结果，且默认视图中这些项立即隐藏。

- Given 归档完成
  When 用户切换到“仅归档”
  Then 可恢复显示这些已归档下架配置。

- Given 用户切换监控页
  When 存在归档下架配置
  Then “最近 24 小时上架”与“已启用监控”两个区块默认隐藏这些归档项，且页面不展示归档筛选控件。

- Given 不同用户访问同一配置
  When 用户 A 执行归档
  Then 用户 B 的该配置 `cleanupAt` 仍为空（归档状态隔离）。

- Given 已归档配置后续重新上架
  When 抓取流程把配置状态恢复到 `active`
  Then 该配置归档记录被自动清理，`cleanupAt` 为空并恢复默认可见。

## 非功能性验收 / 质量门槛（Quality Gates）

- Backend: `cargo fmt`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test --all-features`
- Web: `bun run lint`、`bun run typecheck`、`bun run test:storybook`（若环境可用）

## 实现里程碑（Milestones）

- [x] M1: 新建归档数据表与索引，补齐查询映射 `cleanupAt`
- [x] M2: 新增归档 API（按用户、幂等）
- [x] M3: relist 自动清理归档记录
- [x] M4: 产品页归档弹窗 + 一键归档
- [x] M5: 产品页三态筛选 + 监控页默认隐藏归档
- [x] M6: 测试、Storybook 与验证通过

## 风险 / 假设

- 风险：一次性归档数量较大时，前端确认弹窗需要限制预览数量防止卡顿。
- 假设：重新上架的判定仍以 `lifecycle_state` 回到 `active` 为准。

## 变更记录（Change log）

- 2026-03-03: 初始化规格，冻结“手动全量归档 + 三态筛选 + relist 自动恢复”范围与验收口径。
- 2026-03-03: 完成后端归档表/API、relist 自动清理、产品/监控页三态筛选与归档确认弹窗；本地通过 cargo + web + storybook 全量验证。
- 2026-03-03: review-loop 收敛补丁：归档写入改为单语句 `INSERT ... SELECT ... RETURNING` 缩小竞态窗口；归档后产品刷新失败改为非阻断提示，避免误导“归档失败”。
- 2026-03-03: 需求调整：监控页不再展示归档筛选，仅保留“默认隐藏归档下架项”的只读语义。
