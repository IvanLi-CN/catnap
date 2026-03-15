# HTTP APIs

## POST /api/catalog/refresh/partition

### Purpose

Trigger a forced upstream fetch for one named catalog region (`countryId + regionId`) without starting a global catalog refresh job.

### Request

- Method: `POST`
- Path: `/api/catalog/refresh/partition`
- Body:

```json
{
  "countryId": "7",
  "regionId": "40"
}
```

### Validation

- `countryId` must be non-empty after trim.
- `regionId` must be non-empty after trim.
- `(countryId, regionId)` must exist in current catalog topology as a named region.
- Country scope (`regionId = null`) is invalid for this endpoint.

### Success response

- Status: `200 OK`
- Body:

```json
{
  "countryId": "7",
  "regionId": "40",
  "refreshed": true
}
```

### Error responses

- `400 invalid_argument`
  - missing / empty `countryId`
  - missing / empty `regionId`
  - region scope no longer exists in catalog topology
- `500 internal_error`
  - enqueue / upstream fetch / apply failed

### Behavioral notes

- Server must call `enqueue_and_wait_force_fetch(countryId, Some(regionId), "manual_refresh")`.
- This endpoint does not publish global catalog-refresh SSE progress and does not mutate `/api/catalog/refresh` semantics.
- If the same target is already queued as cache-hit work, this request must upgrade it to force-fetch and wait for the real fetch result.
