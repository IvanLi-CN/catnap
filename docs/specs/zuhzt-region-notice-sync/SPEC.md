# 补齐可用区域说明并替换无效分组文案（#zuhzt）

## 状态

- Status: 已完成
- Created: 2026-03-05
- Last: 2026-03-05

## 背景 / 问题陈述

目标站点在“可用区域”下方提供了分组级说明（例如滥用限制、线路特点）。当前 Catnap 产品页/监控页没有展示这部分关键信息，且产品页分组副标题仍包含固定占位文案（如“长期有货：不提供库存监控开关”），与实际上游信息不一致。

## 目标 / 非目标

### Goals

- 后端从上游页面解析“可用区域说明”，并通过 `/api/bootstrap` 透出。
- 前端在“全部产品/库存监控”分组标题下显示对应说明。
- 删除无效固定分组文案，改为“有上游说明才显示，无说明不占位”。
- 保持分组键对齐：`(countryId, regionId|null)`。

### Non-goals

- 不改库存监控业务规则、轮询策略、下单链路。
- 不引入数据库 schema 迁移；说明仅在内存快照与 bootstrap 返回中传递。
- 不改上游抓取并发与调度策略。

## 范围（Scope）

### In scope

- `src/models.rs`：新增 `RegionNotice`，扩展 `CatalogView`。
- `src/upstream.rs`：新增 `parse_region_notice`；抓取阶段写入 `CatalogSnapshot.region_notices`。
- `src/app_api.rs`：`/api/bootstrap` 返回 `catalog.regionNotices`。
- `src/catalog_refresh.rs` + `src/ops.rs`：按 `(fid,gid)` 更新/清理快照说明，避免陈旧文案残留。
- `web/src/App.tsx`：分组级说明渲染；移除固定副标题逻辑。
- `web/src/stories/fixtures.ts`：补齐 `regionNotices` fixture。
- `tests/api.rs` + `src/upstream.rs` 单测：覆盖接口字段与解析逻辑。

### Out of scope

- 为说明文案新增用户可编辑配置入口。
- 富文本渲染（说明按纯文本归一化显示）。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `catalog.regionNotices[]` | HTTP JSON field | internal | Add | None | backend | web | 结构：`{ countryId, regionId?, text }` |

## 验收标准（Acceptance Criteria）

- Given 上游页面存在“可用区域说明”块
  When 后端解析页面
  Then `parse_region_notice` 返回说明文本，并过滤“📍可用区域”标题块。

- Given 调用 `/api/bootstrap`
  When 返回 catalog 数据
  Then 包含 `regionNotices`，且每项键值与 `(countryId, regionId|null)` 对齐。

- Given 某分组在新一轮抓取中说明缺失
  When 该分组抓取结果应用到快照
  Then 该分组旧说明被清理，不残留陈旧文案。

- Given 产品页或监控页渲染分组
  When 该分组存在 `regionNotices` 映射
  Then 分组标题下显示说明文本。

- Given 分组没有说明
  When 页面渲染
  Then 不显示占位说明，并且不再显示旧固定文案（含“长期有货：不提供库存监控开关”）。

## 非功能性验收 / 质量门槛（Quality Gates）

- Backend: `cargo fmt` + `cargo test --all-features` + `cargo clippy --all-targets --all-features -- -D warnings`
- Web: `cd web && bun run lint` + `cd web && bun run typecheck` + `cd web && bun run test:storybook`

## 实现里程碑（Milestones）

- [x] M1: 扩展后端数据结构与 bootstrap 输出，新增 `regionNotices`。
- [x] M2: 完成上游“可用区域说明”解析并接入抓取链路。
- [x] M3: 完成产品页/监控页分组级说明展示，移除固定无效文案。
- [x] M4: 补齐测试与 Storybook fixture 并通过质量门禁。
- [x] M5: fast-track 收口（PR + checks + review-loop）。

## 风险 / 假设

- 风险：上游 DOM class 或布局变更会导致说明提取失败。
- 假设：说明文案作为纯文本展示即可满足当前业务，不需要保留 HTML 样式。

## 变更记录（Change log）

- 2026-03-05: 初始化规格，冻结范围、接口契约与验收标准。
- 2026-03-05: 完成后端解析与 API 透出、前端分组说明渲染、相关测试与 Storybook fixture 更新。
- 2026-03-05: 完成 review-loop 收敛修复（解析标题变体、按分组初始化 cache bypass、冷启动 notice 回填）并进入 PR 收口阶段。
