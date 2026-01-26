# 数据库（DB）

## Ops observability persistence（ops_* tables）

- 范围（Scope）: internal
- 变更（Change）: New
- 影响表（Affected tables）: `ops_events`, `ops_task_runs`, `ops_notify_runs`

### Schema delta（结构变更）

（DDL 仅表达形状，具体 SQLx 迁移/创建逻辑在实现阶段落地；以下字段用于实现成功率/渠道成功率与 SSE 续传。）

- Tables:
  - `ops_events`（7 天留存；SSE 回放限制 1 小时由服务端逻辑保证）
    - `id INTEGER PRIMARY KEY AUTOINCREMENT`
    - `ts TEXT NOT NULL`（RFC3339）
    - `event TEXT NOT NULL`（例如 `ops.task`/`ops.log`）
    - `data_json TEXT NOT NULL`
  - `ops_task_runs`（采集任务运行记录，用于成功率统计与任务追踪）
    - `id INTEGER PRIMARY KEY AUTOINCREMENT`
    - `fid TEXT NOT NULL`
    - `gid TEXT NULL`
    - `started_at TEXT NOT NULL`
    - `ended_at TEXT NULL`
    - `ok INTEGER NOT NULL`（0/1；ok=“抓取+解析成功”）
    - `fetch_http_status INTEGER NULL`
    - `fetch_bytes INTEGER NULL`
    - `fetch_elapsed_ms INTEGER NULL`
    - `parse_produced_configs INTEGER NULL`
    - `parse_elapsed_ms INTEGER NULL`
    - `error_code TEXT NULL`
    - `error_message TEXT NULL`
  - `ops_notify_runs`（推送发送记录，用于渠道成功率统计）
    - `id INTEGER PRIMARY KEY AUTOINCREMENT`
    - `task_run_id INTEGER NOT NULL`
    - `ts TEXT NOT NULL`
    - `channel TEXT NOT NULL`（`telegram|webPush`）
    - `result TEXT NOT NULL`（`success|error|skipped`）
    - `error_message TEXT NULL`
- Indexes（建议）:
  - `ops_events(ts DESC, id DESC)`
  - `ops_task_runs(ended_at DESC, id DESC)`
  - `ops_task_runs(fid, gid, ended_at DESC)`
  - `ops_notify_runs(task_run_id)`
  - `ops_notify_runs(channel, ts DESC)`

### Migration notes（迁移说明）

- 向后兼容窗口（Backward compatibility window）:
  - 新表新增不影响现有功能；ops 页面仅依赖新表。
- 发布/上线步骤（Rollout steps）:
  - 先落表结构与清理逻辑，再上线 ops API 与 UI。
- 回滚策略（Rollback strategy）:
  - 回滚时可保留新表不使用；不影响现有 `/api/*` 业务路径。
- 回填/数据迁移（Backfill / data migration）:
  - None（不对既有 event_logs 回填）。

