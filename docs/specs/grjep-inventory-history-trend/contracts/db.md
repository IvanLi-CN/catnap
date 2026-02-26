# 数据库（DB）

本计划为库存走势新增“按分钟”的历史表，用于支撑近 24h 的查询与可视化。

## Inventory history samples（minute）

- 范围（Scope）: internal
- 变更（Change）: New
- 影响表（Affected tables）: `inventory_samples_1m`

### Schema delta（结构变更）

- DDL / migration snippet（示意；实现以 SQLite 为准）:
  - `inventory_samples_1m`:
    - `config_id` TEXT NOT NULL
    - `ts_minute` TEXT NOT NULL（RFC3339，minute-aligned）
    - `inventory_quantity` INTEGER NOT NULL（>= 0）
    - PRIMARY KEY (`config_id`, `ts_minute`)
- Constraints / indexes:
  - 追加 index（可选）：`CREATE INDEX idx_inventory_samples_1m_ts ON inventory_samples_1m(ts_minute);`（用于保留期清理）

写入规则（待冻结；本计划默认建议）：

- 每次抓取/轮询得到配置的 `checked_at` 后：
  - 归一化到 `ts_minute = floor_to_minute(checked_at)`
  - 执行 upsert（同一 `config_id + ts_minute` 只保留一条记录；若一分钟内多次采样，以“最后一次”为准）

### Migration notes（迁移说明）

- 向后兼容窗口（Backward compatibility window）:
  - 新表为增量新增，不影响既有 schema 读取；实现时仍需考虑“旧库无此表”的自举创建策略。
- 发布/上线步骤（Rollout steps）:
  - 上线后开始写入；UI 在历史接口返回空/无数据时应降级显示占位态。
- 回滚策略（Rollback strategy）:
  - 回滚到旧版本时忽略该表即可；不应阻塞启动。
- 回填/数据迁移（Backfill / data migration, 如适用）:
  - 不做历史回填；仅从上线后开始积累。

保留期与清理（待决策）：

- 保留期：暂定 30 天。
- 清理策略建议在轮询周期内做“按时间删除”：
  - `DELETE FROM inventory_samples_1m WHERE ts_minute < <cutoff>`（`cutoff = now - 30d`）
