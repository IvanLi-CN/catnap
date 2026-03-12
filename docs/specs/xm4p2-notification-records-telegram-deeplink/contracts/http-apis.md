# HTTP APIs（#xm4p2）

## GET /api/notifications/records

用途：按时间倒序返回通知记录页所需的通知组分页结果。

### Query

- `cursor`: `string | null`
  - 格式：`<RFC3339 createdAt>:<id>`
  - 缺省时表示从最新记录开始。
  - 非法值返回 `400 INVALID_ARGUMENT`。
- `limit`: `number | null`
  - 缺省 `20`
  - 最小 `1`
  - 最大 `50`

### 200 Response

```json
{
  "items": [
    {
      "id": "5a3bbf4d-0b73-46e6-b7bb-6a29dbb45d92",
      "createdAt": "2026-03-11T08:00:00Z",
      "kind": "monitoring.restock+price",
      "title": "补货 + 价格变动",
      "summary": "HKG-Pro.TRFC Pro · 库存 0 → 1｜价格 ¥99.00 → ¥90.00 / 月",
      "partitionLabel": "中国香港 / HKG",
      "telegramStatus": "success",
      "webPushStatus": "skipped",
      "items": [
        {
          "configId": "lc:7:40:abc",
          "name": "HKG-Pro.TRFC Pro",
          "countryName": "中国香港",
          "regionName": "HKG",
          "partitionLabel": "中国香港 / HKG",
          "specs": [{ "key": "CPU", "value": "2 vCPU" }],
          "price": { "amount": 90, "currency": "CNY", "period": "month" },
          "inventory": {
            "status": "available",
            "quantity": 1,
            "checkedAt": "2026-03-11T08:00:00Z"
          },
          "lifecycle": {
            "state": "active",
            "listedAt": "2026-03-01T00:00:00Z"
          }
        }
      ]
    }
  ],
  "nextCursor": "2026-03-11T07:59:00Z:2f0ec4b4-2fa0-4a40-9152-17b3850e9e40"
}
```

### 400

```json
{
  "error": {
    "code": "INVALID_ARGUMENT",
    "message": "cursor 必须是 <RFC3339>:<id>"
  }
}
```

### Notes

- `items[]` 始终返回数组，即使仅有 1 条机子快照。
- 列表按 `createdAt DESC, id DESC` 排序。
- `kind` 当前值域：
  - `monitoring.restock`
  - `monitoring.price`
  - `monitoring.config`
  - `monitoring.restock+price`
  - `monitoring.restock+config`
  - `monitoring.price+config`
  - `monitoring.restock+price+config`
  - `catalog.partition_listed`
  - `catalog.site_listed`
  - `catalog.delisted`
- `telegramStatus` / `webPushStatus` 值域：`pending | success | skipped | error | not_sent`。

## GET /api/notifications/records/:id

用途：按通知 ID 返回单条通知组，供 Telegram 深链预取/定位。

### 200 Response

返回值结构与列表中的单个 `item` 完全一致。

### 404

```json
{
  "error": {
    "code": "NOT_FOUND",
    "message": "记录不存在或已过期"
  }
}
```

## Telegram deep link contract

真实通知在 `siteBaseUrl` 存在时追加：

```text
查看通知记录：https://<site>/?notification=<record_id>#notifications
```

- 保留现有“查看监控：<base>/monitoring”行，并在其后追加通知记录深链。
- 仅适用于真实业务通知：监控变化、上新、下架。
- 测试通知不追加该行。
- `siteBaseUrl` 为空时整行省略。
