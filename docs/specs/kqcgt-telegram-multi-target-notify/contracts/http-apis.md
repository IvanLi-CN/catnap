# HTTP API Contracts

## Settings / Bootstrap Telegram shape

- `SettingsView.notifications.telegram`
  - before: `{ enabled: boolean, configured: boolean, target?: string }`
  - after: `{ enabled: boolean, configured: boolean, targets: string[] }`
- `SettingsUpdateRequest.notifications.telegram`
  - after:
    - `enabled: boolean`
    - `botToken: string | null`
    - `targets: string[] | null`

Rules:
- server trims each target, removes empty values, preserves first-seen order, and deduplicates exact matches after trim;
- read path falls back to legacy `telegram_target` when `telegram_targets_json` is empty/null;
- write path mirrors the first normalized target back to `telegram_target` for legacy compatibility.

## POST /api/notifications/telegram/test

Request body:

```json
{
  "botToken": "123456:abc" ,
  "targets": ["@catnap", "-1002233445566"],
  "text": null
}
```

Response on full success:

```json
{
  "ok": true,
  "status": "success",
  "results": [
    { "target": "@catnap", "status": "success" },
    { "target": "-1002233445566", "status": "success" }
  ]
}
```

Response on partial success:

```json
{
  "ok": false,
  "status": "partial_success",
  "results": [
    { "target": "@catnap", "status": "success" },
    { "target": "-1002233445566", "status": "error", "error": "telegram http 400: chat not found" }
  ]
}
```

Rules:
- if request `targets` is null/empty after normalization, server falls back to saved targets;
- if all effective targets are missing, return `400 INVALID_ARGUMENT`;
- if at least one target succeeds and at least one fails, return `200` with `status=partial_success`;
- if all targets fail, return `500 INTERNAL` with `status=error` and per-target results;
- response must never include bot token.

## Notification records

`NotificationRecordView` adds:

```json
{
  "telegramStatus": "success | partial_success | error | skipped | pending | not_sent",
  "telegramDeliveries": [
    {
      "channel": "telegram",
      "target": "@catnap",
      "status": "success | error | skipped | pending | not_sent",
      "error": null
    }
  ]
}
```

Rules:
- `telegramDeliveries` is omitted or empty only when Telegram was not attempted for that record;
- deliveries are returned in the same order as the normalized target list used for sending;
- `webPushStatus` contract is unchanged.
