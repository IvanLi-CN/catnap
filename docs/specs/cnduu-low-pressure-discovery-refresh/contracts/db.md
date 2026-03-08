# DB Contract

## 目标

支撑 `DB-first` 启动、`topology_refresh` / `discovery_due` 渐进扫描，以及各类 reason 的 freshness reuse。

## Catalog snapshot bootstrap

- 服务启动读取本地 DB 时，必须能同时恢复：
  - catalog configs 当前视图；
  - 已知国家/可用区拓扑；
  - 已知 `url_key` 最近一次成功抓取时间。
- 非空 DB 启动场景下，读取这些数据即可对外服务，不要求先请求上游。

## URL success metadata

- `catalog_url_cache` 继续作为单个 `url_key` 最近成功抓取的持久化来源。
- 语义补充：
  - `last_success_at` 用于 freshness window 命中判断；
  - `config_ids_json` 继续作为 lifecycle diff 的基线；
  - 失败抓取或解析失败不得覆盖该记录。
- freshness window 固定值：
  - `poller_due = 45s`
  - `discovery_due = 150s`
  - `manual_refresh = 300s`
  - `topology_refresh = 1800s`

## Topology metadata

- 启动与低频拓扑复扫必须能够恢复/更新：
  - countries（`fid`）
  - regions（`gid`）
  - `url_key = fid:gid|0`
- 空库启动只要求写入 root/fid 枚举出来的拓扑；各 `gid/url_key` 的完整页面抓取由 `discovery_due` 完成。
- 拓扑刷新不得直接触发“对所有已知 `url_key` 立即发起真实抓取”。

## Lifecycle consistency

- listed/delisted/relisted 判定继续以成功抓取后的 fetched set 与 `catalog_url_cache.config_ids_json` 做差异为准。
- `discovery_due`、`poller_due`、`manual_refresh` 成功抓取后的 apply 逻辑必须共用同一套状态迁移规则。
- relisted 仍视为 listed 事件，计入 `recentListed24h` 查询结果。

## PID recovery boundary

- `source_pid` 可继续保留在 catalog config 记录中。
- 但启动、discovery、monitoring 热路径不得依赖 `configureproduct` 页面批量探测来补齐 `source_pid`。
- 若后续需要低预算补全 `source_pid`，必须作为独立后台链路，不影响本计划的发现 SLA。
