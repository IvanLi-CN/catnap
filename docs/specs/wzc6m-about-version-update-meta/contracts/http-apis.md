# HTTP APIs Contracts（#wzc6m）

## `GET /api/about`

- Scope: internal
- Change: New
- Auth: required

### Query

- `force` (optional):
  - `force=1`: bypass TTL and refresh update-check cache

### Response (200)

Content-Type: `application/json`

Body shape:

```json
{
  "version": "0.1.0",
  "webDistBuildId": "abcdef",
  "repoUrl": "https://github.com/IvanLi-CN/catnap",
  "update": {
    "enabled": true,
    "status": "ok",
    "checkedAt": "2026-02-17T00:00:00Z",
    "latestVersion": "0.1.9",
    "latestUrl": "https://github.com/IvanLi-CN/catnap/releases/tag/v0.1.9",
    "updateAvailable": false,
    "message": null
  }
}
```

Fields:

- `version` (string, required): current effective version (semver or other label)
- `webDistBuildId` (string, required): build id for embedded `web/dist`
- `repoUrl` (string, required): repository URL displayed in UI
- `update` (object, required):
  - `enabled` (bool, required): whether update-check is enabled
  - `status` (string, required): one of `ok|disabled|error`
  - `checkedAt` (string|null): RFC3339 timestamp of last check attempt
  - `latestVersion` (string|null): latest stable semver (no leading `v`)
  - `latestUrl` (string|null): GitHub release URL (html_url)
  - `updateAvailable` (bool, required): best-effort comparison result
  - `message` (string|null): short error message for UI (no stack traces)

