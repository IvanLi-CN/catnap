# 数据库（DB）

本仓库当前使用 `init_db()` 内联建表（无显式 migration 文件）。本计划需要扩展 schema：新增列与新增表；要求保持向后兼容（旧数据可继续使用）。

## 配置生命周期（catalog_configs）

- 范围（Scope）: internal
- 变更（Change）: Modify
- 影响表（Affected tables）: `catalog_configs`

### Schema delta（结构变更）

- 新增列（建议）：
  - `lifecycle_state TEXT NOT NULL DEFAULT 'active'`（`active|delisted`）
  - `first_seen_at TEXT NOT NULL DEFAULT <now>`（首次上架时间）
  - `last_seen_at TEXT NOT NULL DEFAULT <now>`（最近一次在上游“成功抓取结果”中出现的时间）
  - `listed_at TEXT NOT NULL DEFAULT <now>`（最近一次从 delisted→active 的时间；首次上架同样写入）
  - `delisted_at TEXT NULL`（下架时间；active 时为 NULL）

### Migration notes（迁移说明）

- 发布/上线步骤：
  1) 先发布包含新列的版本（`ALTER TABLE` 或重建表策略，取决于当前实现方式）
  2) 默认将历史数据视为 `active`，并用 `checked_at` 回填 `first_seen_at/last_seen_at/listed_at`
- 回滚策略：
  - 若需回滚到旧版本，旧代码必须忽略新增列（SQLite 允许）。

## URL last good result（upstream_url_snapshots）

- 范围（Scope）: internal
- 变更（Change）: New
- 影响表（Affected tables）: new table

### Schema delta（结构变更）

- 新表（建议）：
  - `url_key TEXT PRIMARY KEY`（归一化 key，例如 `fid:gid`，无 gid 用 `0`）
  - `url TEXT NOT NULL`
  - `fetched_at TEXT NOT NULL`
  - `config_ids_json TEXT NOT NULL`（JSON array of strings）
  - `digest TEXT NOT NULL`（用于快速判断集合变化）

### Migration notes（迁移说明）

- 该表用于：
  - 为“全量刷新”提供 cache hit 的依据（无需重复抓取也能推进 job）
  - 用于 listed/delisted 差异计算（以 last good snapshot 作为基线）
- 失败抓取不得覆盖该表（仅成功抓取才更新）。

## 最近 24 小时上架查询（recentListed24h）

- 不新增专用表：以 `catalog_configs.listed_at` 为查询依据
  - `listed_at >= now - 24h` → “最近 24 小时上架”
