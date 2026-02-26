# UI 线框图与交互说明（Wireframes）

本目录描述“采集观测台（Ops）”页面的信息架构与关键交互口径，用于实现阶段对齐 UI 与 SSE 数据形状（不绑定具体框架实现）。

页面：

- `ops-dashboard.svg`：采集观测台（全局队列 + workers + 统计 + 实时日志 tail）
- `sse-tooltip.svg`：SSE 连接状态悬浮气泡（单独示意，忽略无关内容）

## 信息架构

### 顶栏（Top bar）

- 标题：`Catnap • 采集观测台`
- 状态 pill（只表达用户可理解的信息）：
  - SSE 连接状态：仅展示“状态点 + SSE”短标识；详细信息（replay window、Last-Event-ID、reset 原因）以悬浮气泡展示
  - `range` 选择：`24h / 7d / 30d`（影响统计口径与 metrics 事件）
  - “跟随滚动”：`On/Off`（仅影响日志 tail 视图，不影响数据订阅）

### 主体分区（Main）

1) **概览（Overview cards）**

- 视觉口径：KPI 卡片（上边缘彩色强调条 + 左上角图标点 + 大号主数值 + 次级指标 + 底部微型趋势线 sparkline）。
- Queue：pending/running、合并次数（deduped）、最近队列更新时间。
- Collection success rate：按 `range` 的成功率与分子/分母（只算抓取+解析）。
- Notify rates：Telegram/Web Push 分渠道成功率与最近失败原因摘要（不混入 collection success rate）。
- Volume：当前 `range` 内的任务量与平均速率（由 `stats.collection.total` 推导）；失败数只作补充信息。

（说明：replay window / Last-Event-ID 不作为 KPI 卡片展示；仅在 SSE 状态悬浮气泡中出现。）

2) **Workers**

- 展示 worker 并发（默认 2，可配置）。
- 每个 worker 显示：
  - 两行布局：
    - 第一行：状态点（圆点 badge）+ 工作者名称（可在最宽视口直接显示状态文字；窄视口只显示圆点，状态用悬浮提示）
    - 第二行：当前任务（key/阶段）+ 耗时；空闲时显示 `当前：-` 与最近错误摘要

3) **Queue tasks（pending/running）**

- 列表项为 `(fid,gid)` 任务：
  - key：`fid` / `gid`（`gid` 为空时显示 `-`）
  - 状态列：默认只显示“圆点 badge”（hover 显示状态 overtip）；在项目定义的“最宽视口”下可在圆点右侧直接展示状态文字
  - enqueuedAt
  - reasonCounts：常规视图做紧凑展示（可能缩写/省略），hover 显示完整内容
  - lastRun（endedAt + ok）
- 不展示触发者 user id（页面对普通用户可见）。

4) **Live log tail（最近 N 条，自动滚动）**

- 默认自动跟随到底部（follow）。
- 当用户手动上滚时：
  - 进入“暂停跟随”状态，停止自动滚动；
  - 显示提示与按钮：`跳到底部` / `恢复跟随`。
- 搜索输入：使用“搜索：关键字…”的输入框（不使用窄按钮），用于过滤 `scope/message`。
- 日志必须覆盖：
  - 采集任务 start/end、抓取结果、解析结果；
  - “成果”（restock/price/config 等）；
  - 推送触发与结果（成功/失败都写）。

## 关键交互口径

### SSE 续传与 reset

- 初次进入页面：
  - 先拉 `GET /api/ops/state?range=...` 获取 snapshot + tail；
  - 再建立 SSE `GET /api/ops/stream?range=...`。
- 断线重连：
  - 以最近一次收到的 `eventId` 作为 `Last-Event-ID` 重连；
  - 若收到 `ops.reset`，客户端必须执行：重拉 snapshot，并以“无 Last-Event-ID”重连。

### range 选择

- `range` 改变只影响统计口径（success rates/metrics）与 snapshot；
- 日志 tail 为实时 + 持久化最近 N 条，仍按事件时间排序展示。

### 错误展示

- “解析失败”必须被可见化：
  - 在任务事件/日志里显示 `error.code` 与短 message；
  - 计入失败率。
