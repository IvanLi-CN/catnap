# 配置（Config）

本计划新增/使用的运行参数（env vars）。所有参数均为 **internal**，默认值用于安全上线。

## Ops runtime knobs

- 范围（Scope）: internal
- 变更（Change）: New

| Name | Type | Default | Notes |
| --- | --- | --- | --- |
| `CATNAP_OPS_WORKER_CONCURRENCY` | integer | `2` | worker 并发数；建议最小 `1` |
| `CATNAP_OPS_SSE_REPLAY_WINDOW_SECONDS` | integer | `3600` | 断线续传最大回放窗口（秒） |
| `CATNAP_OPS_LOG_RETENTION_DAYS` | integer | `7` | ops 事件/运行记录保留天数（至少 7） |
| `CATNAP_OPS_LOG_TAIL_LIMIT_DEFAULT` | integer | `200` | snapshot 默认 `logLimit` |
| `CATNAP_OPS_QUEUE_TASK_LIMIT_DEFAULT` | integer | `200` | snapshot 默认 `taskLimit` |

