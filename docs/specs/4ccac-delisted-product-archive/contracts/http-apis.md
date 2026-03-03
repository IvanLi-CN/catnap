# HTTP APIs

## POST `/api/products/archive/delisted`

- Auth: required (`x-user`)
- Semantics: archive all configs where `lifecycle_state='delisted'` and the current user has no archive row.
- Idempotency: yes. Repeated calls archive only newly matched rows.

### Response `200`

```json
{
  "archivedCount": 3,
  "archivedAt": "2026-03-03T12:34:56Z",
  "archivedIds": ["lc:7:40:127", "lc:7:40:128", "lc:2:56:117"]
}
```

`archivedAt` is omitted when `archivedCount=0`.

## Existing config payload extension

`Config.lifecycle` adds optional field:

- `cleanupAt?: string` (RFC3339 UTC)
  - Present only when the current user archived this config while it is delisted.
  - Cleared automatically once the config is relisted.
