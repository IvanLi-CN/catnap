# 事件（Events）

本计划使用 SSE（Server-Sent Events）对前端实时发布全量刷新 job 状态。

## catalog.refresh（SSE event）

- 范围（Scope）: internal
- 变更（Change）: New
- 生产者（Producer）: backend
- 消费者（Consumers）: web（所有在线用户客户端）
- 投递语义（Delivery semantics）: at-least-once；无严格 ordering 保证（前端按 `updatedAt`/`jobId` 去重/覆盖）

### 载荷（Payload）

- Schema（JSON）:
  ```json
  {
    "jobId": "uuid",
    "state": "idle|running|success|error",
    "trigger": "manual|auto",
    "done": 3,
    "total": 12,
    "message": null,
    "startedAt": "2026-01-23T00:00:00Z",
    "updatedAt": "2026-01-23T00:00:00Z",
    "current": {
      "urlKey": "fid:gid",
      "url": "https://example.invalid/cart?fid=7&gid=56",
      "action": "fetch|cache",
      "note": "optional string"
    }
  }
  ```

### 兼容性规则（Compatibility rules）

- 增量字段（Additive changes）允许；前端必须忽略未知字段。
- 字段删除/重命名需要一个兼容周期（至少一个版本）并在计划中显式声明。
