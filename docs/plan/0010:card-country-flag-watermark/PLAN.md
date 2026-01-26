# 配置卡片：国家国旗水印背景（#0010）

## 状态

- Status: 待实现
- Created: 2026-01-25
- Last: 2026-01-25

## 背景 / 问题陈述

- 当前“配置卡片 / 监控卡片”主要依赖分组标题与文字信息来体现国家/地区；在卡片网格快速扫视时，国家信息不够直观。
- 希望在卡片左上角增加“国旗水印”作为半透明背景弱提示，兼具装饰性，但不影响内容可读性与现有趋势背景（sparkline）。
- 现有数据形状提示：后端抓取上游时 `Country.id` 来自上游页面的 `fid`（数字字符串），并非 ISO2 国家码；因此需要通过 `Country.name`（例如“日本/美国”）或显式映射来确定旗帜。

## 目标 / 非目标

### Goals

- 在配置卡片与监控卡片左上角展示与国家匹配的“国旗水印背景”（半透明、非交互）。
- 水印仅作弱提示：不改变卡片信息结构、不承担强信息表达职责。
- 不引入运行时网络请求；对渲染性能影响可忽略。
- 亮/暗主题下均保持良好可读性。

### Non-goals

- 不新增/修改国家数据源、国家筛选逻辑、或国家/区域的显示文案。
- 不在列表/面板标题等其他位置新增国旗展示（仅限卡片本体）。
- 不做“按区域（Region）展示旗帜/徽章”等扩展。

## 范围（Scope）

### In scope

- `ProductCard`（`.cfg-card`）与 `MonitoringCard`（`.mon-card`）增加国旗水印背景。
- 根据 `cfg.countryId` 推导国旗展示；对无法识别的 `countryId` 采取降级（不展示水印）。

### Out of scope

- 其他卡片类型（若存在）或其他页面组件的旗帜展示。
- 旗帜资源的国际化/政治语义讨论；本计划仅做“视觉弱提示”。

## 需求（Requirements）

### MUST

- 配置卡片与监控卡片左上角存在国旗水印背景：
  - 位置：卡片左上角（随卡片滚动/布局，固定在卡片内部）；水印层与卡片边缘距离为 `0`（full-bleed，贴边）。
  - 视觉：半透明（watermark），不遮挡正文可读性；不影响右上角大号数量水印（`cfg-cap`/`mon-cap`）与趋势背景（`TrendBackground`）的存在感。
  - 透明度渐变：水印的可见度从左上到右下逐渐变淡（对角渐变），目标为左上 `0.4`、右下 `0`（这里的数值指“水印最终 alpha”，而不是再乘一个 base opacity；实现时 base opacity 应为 `1`，通过 mask/gradient 直接得到 `0.4 → 0`）。
  - 水印尺寸：水印只占据左上角一块区域（不铺满整张卡片）；尺寸在实现阶段以“弱提示、不干扰正文”为准微调。
  - 交互：不可点击、不响应 hover、不影响卡片内部控件交互。
- 国旗水印与国家的对应关系：
  - 输入：`cfg.countryId` + `bootstrap.catalog.countries`（通过国家列表将 `countryId` 解析为国家名，再映射到旗帜）。
  - 规则：当且仅当“国家名可映射到某个旗帜 icon”时展示；否则不展示。
  - 资源：使用 Iconify 图标（旗帜类 icon），不引入运行时网络请求。
  - Icon set：选择 `flagpack`（矩形旗帜；Iconify prefix=`flagpack`）。
  - 未知国家/特殊类目：
    - `Country.name` 包含“云服务器”或等价非国家类目：不显示水印
    - 其他无法映射：不显示水印（静默降级）
- 兼容与可读性：
  - 同时支持亮色/暗色主题，文本与关键 UI（标题、规格、价格、监控按钮/更新时间）可读性不下降。
  - 水印属于纯装饰，不应被屏幕阅读器读出（不新增可聚焦元素/可访问名称）。

## 接口契约（Interfaces & Contracts）

None（仅 UI 表现变更，不新增/修改/删除对外接口）。

## 验收标准（Acceptance Criteria）

- Given 产品列表中存在“国家名=日本”的配置卡片
  When 渲染卡片
  Then 卡片左上角显示日本国旗水印背景，且正文信息可读性不受影响

- Given 监控列表中存在“国家名=美国”的监控卡片
  When 渲染卡片
  Then 卡片左上角显示美国国旗水印背景，且“更新：xx”与价格等信息保持清晰

- Given `cfg.countryId` 无法解析出国家名，或国家名无法映射到旗帜 icon
  When 渲染卡片
  Then 不展示国旗水印（不出现异常占位图/布局抖动）

- Given 切换到暗色主题
  When 渲染上述卡片
  Then 国旗水印仍为弱提示（不过亮/不过花），不降低正文对比度

- Given 渲染带水印的卡片
  When 观察国旗水印
  Then 水印在左上更清晰、右下逐渐淡至不可见（对角渐变），与目标值（左上 `~0.4` / 右下 `0` 的最终 alpha）一致

## 实现前置条件（Definition of Ready / Preconditions）

- Icon set 已冻结为 Iconify `flagpack`（矩形旗帜）。
- 降级策略已冻结：云服务器/未知国家不显示水印。
- 已完成最小必要事实核查：已从上游页面获取国家列表并冻结映射（见下方“国家映射（Frozen mapping）”）。

## 非功能性验收 / 质量门槛（Quality Gates）

### UI / Storybook

- 更新/新增 Storybook stories 覆盖至少两种国家（例如 `jp` / `us`）与一个不可映射的 `countryId` 降级场景。
- `cd web && bun run test:storybook` 通过（含 Playwright 运行的 story 测试）。

### Quality checks

- `cd web && bun run lint` 通过（Biome）。
- `cd web && bun run typecheck` 通过。

## 文档更新（Docs to Update）

- `docs/ui/cards.svg`：新增卡片组件设计图（ProductCard / MonitoringCard，含国旗水印）。
- `docs/ui/README.md`：把 `cards.svg` 加入清单与来源映射。

## 资产晋升（Asset promotion）

None（计划默认优先采用“无需新增旗帜资源”的实现路径；若改为引入 SVG/PNG 旗帜资源，需要在实现阶段补齐该表并完成晋升）。

## UI 草图（Plan-only）

- `ui/cards.svg`：UI 组件设计图（高保真）：ProductCard / MonitoringCard（含国旗水印，Light/Dark）（评审基准）。本版已改为基于实际 CSS token（`web/src/app.css`）与真实组件结构（`ProductCard`/`MonitoringCard`）重画：贴边水印、最终 alpha 左上 `0.4` → 右下 `0`，且水印不铺满整张卡片。
- `assets/products-flag-watermark.svg`：基于 `docs/ui/products.svg` 的界面设计，叠加“国旗水印”效果（辅助验证：不破坏整体布局）。
- `assets/inventory-monitor-flag-watermark.svg`：基于 `docs/ui/inventory-monitor.svg` 的界面设计，叠加“国旗水印”效果（辅助验证：不破坏整体布局）。

## 国家映射（Frozen mapping）

来源：抓取上游 `https://lazycats.vip/cart`（2026-01-25）首页左侧国家列表（`.firstgroup_item`），并按 `Country.name` 做到 ISO2 的显式映射；除“云服务器”外其余条目均可映射到 `flagpack:<iso2>`。

| Country.name（国家名） | ISO2 | Iconify icon |
| --- | --- | --- |
| 南极洲 | aq | flagpack:aq |
| 朝鲜 | kp | flagpack:kp |
| 格陵兰 | gl | flagpack:gl |
| 新加坡 | sg | flagpack:sg |
| 日本 | jp | flagpack:jp |
| 中国台湾 | tw | flagpack:tw |
| 中国香港 | hk | flagpack:hk |
| 美国 | us | flagpack:us |
| 冰岛 | is | flagpack:is |
| 加拿大 | ca | flagpack:ca |
| 爱尔兰 | ie | flagpack:ie |
| 奥地利 | at | flagpack:at |
| 俄罗斯 | ru | flagpack:ru |
| 乌克兰 | ua | flagpack:ua |
| 瑞士 | ch | flagpack:ch |
| 英国 | gb | flagpack:gb |
| 德国 | de | flagpack:de |
| 芬兰 | fi | flagpack:fi |
| 印度 | in | flagpack:in |
| 土耳其 | tr | flagpack:tr |
| 越南 | vn | flagpack:vn |

## 方案概述（Approach, high-level）

- 采用“卡片容器叠加装饰层”的方式实现水印（例如 CSS pseudo-element），位于卡片背景层与正文之间。
- 国旗映射在组件层完成（避免 CSS 侧进行数据逻辑）：
  - `countryId -> countryName`（通过 `bootstrap.catalog.countries`）
  - `countryName -> flagIcon`（稳定映射表；未知则不显示）
  - 通过 `data-*` 或 CSS 变量向样式层传递“可渲染的水印信息”。
- 预期改动点（实现阶段）：
  - 组件：`web/src/App.tsx`（`ProductCard` / `MonitoringCard`）
  - 样式：`web/src/app.css`（`.cfg-card` / `.mon-card` 的装饰层）
  - Storybook：`web/src/stories/components/*.stories.tsx` + `web/src/stories/fixtures.ts`
  - 依赖：`web/package.json`（新增 Iconify 相关依赖；确保无运行时网络请求）

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：`flagpack` icon set 引入方式会影响 bundle 体积；需要确保不走在线 API（无运行时网络请求）。
- 风险：上游国家列表未来新增/改名时，需要补齐映射表；未映射的国家将按约定静默不显示水印。

## 变更记录（Change log）

- 2026-01-25: 冻结方案：Iconify `flagpack`（矩形）+ 显式国家名映射 + 云/未知不显示；补齐 UI 草图。
- 2026-01-25: 冻结透明度渐变口径为“水印最终 alpha”（非乘数）：左上 `~0.4` → 右下 `0`（实现时 base opacity=1，通过 mask/gradient 直接得到 `0.4 → 0`）。
- 2026-01-25: UI 草图改为基于 `docs/ui/*.svg` 的原始卡片设计（用于校准真实布局与层级）。
- 2026-01-25: 增加 UI 组件高保真设计图 `ui/cards.svg` 作为评审基准。
- 2026-01-25: 组件设计图需继续对齐“最新实际 UI”截图后再进入实现。

## 参考（References）

- `web/src/App.tsx`：`ProductCard` / `MonitoringCard`
- `web/src/app.css`：`.cfg-card` / `.mon-card` 相关样式
- `web/src/stories/components/ProductCard.stories.tsx`
- `web/src/stories/components/MonitoringCard.stories.tsx`
