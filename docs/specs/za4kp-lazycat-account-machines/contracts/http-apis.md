# HTTP API Contracts

## BootstrapResponse

`BootstrapResponse` adds:

```json
{
  "lazycat": {
    "connected": true,
    "state": "authenticating | syncing | ready | error | disconnected",
    "machineCount": 7,
    "lastSiteSyncAt": "2026-03-19T14:00:00Z",
    "lastPanelSyncAt": "2026-03-19T14:05:00Z",
    "lastError": null
  }
}
```

Rules:

- when no lazycat account is bound, `connected=false`, `state="disconnected"`, `machineCount=0`;
- `lastError` is omitted or null when there is no active error;
- `machineCount` comes from cached machines for the current Catnap user only.

## GET /api/lazycat/account

Response body:

```json
{
  "connected": true,
  "email": "user@example.com",
  "state": "ready",
  "machineCount": 7,
  "lastSiteSyncAt": "2026-03-19T14:00:00Z",
  "lastPanelSyncAt": "2026-03-19T14:05:00Z",
  "lastError": null
}
```

Rules:

- password and cookies are never returned;
- if no account is bound, response still returns `connected=false` summary instead of `404`.

## POST /api/lazycat/account/login

Request body:

```json
{
  "email": "user@example.com",
  "password": "secret"
}
```

Response body:

```json
{
  "connected": true,
  "email": "user@example.com",
  "state": "syncing",
  "machineCount": 0,
  "lastSiteSyncAt": null,
  "lastPanelSyncAt": null,
  "lastError": null
}
```

Rules:

- successful login persists credentials + cookies and triggers immediate sync;
- invalid credentials return `400 INVALID_ARGUMENT` with actionable message;
- switching account reuses the same endpoint; server overwrites the previous bound account for the current user.

## DELETE /api/lazycat/account

Response body:

```json
{
  "ok": true
}
```

Rules:

- server deletes the current user’s lazycat account row, cached machines, and cached port mappings;
- endpoint is idempotent.

## POST /api/lazycat/sync

Response body:

```json
{
  "connected": true,
  "email": "user@example.com",
  "state": "syncing",
  "machineCount": 7,
  "lastSiteSyncAt": "2026-03-19T14:00:00Z",
  "lastPanelSyncAt": "2026-03-19T14:05:00Z",
  "lastError": null
}
```

Rules:

- endpoint does not change credentials;
- if no account is bound, return `400 INVALID_ARGUMENT`;
- manual sync schedules or executes a full site + panel refresh for the current user.

## GET /api/lazycat/machines

Response body:

```json
{
  "account": {
    "connected": true,
    "email": "user@example.com",
    "state": "ready",
    "machineCount": 7,
    "lastSiteSyncAt": "2026-03-19T14:00:00Z",
    "lastPanelSyncAt": "2026-03-19T14:05:00Z",
    "lastError": null
  },
  "items": [
    {
      "serviceId": 2312,
      "serviceName": "港湾 Transit Mini",
      "serviceCode": "srvQ8L2M5R1P9K",
      "status": "Active",
      "os": "Alpine-3.20-amd64",
      "primaryAddress": "edge-node-24.example.net",
      "extraAddresses": [],
      "expiresAt": "2026-04-11T12:24:42Z",
      "billingCycle": "monthly",
      "renewPrice": "¥9.34元/月付",
      "firstPrice": "¥9.34元",
      "traffic": {
        "usedGb": 700.22,
        "limitGb": 800,
        "resetDay": 11,
        "lastResetAt": "2026-03-10T16:00:08.774266055Z",
        "display": "700.22 GB / 800 GB"
      },
      "portMappings": [
        {
          "family": "v4",
          "publicIp": "192.168.9.104",
          "publicPort": 52222,
          "publicPortEnd": 52222,
          "privateIp": "10.0.55.96",
          "privatePort": 22,
          "privatePortEnd": 22,
          "protocol": "tcp",
          "status": "active",
          "description": ""
        }
      ],
      "lastSiteSyncAt": "2026-03-19T14:00:00Z",
      "lastPanelSyncAt": "2026-03-19T14:05:00Z",
      "detailState": "ready",
      "detailError": null
    }
  ]
}
```

Rules:

- `items` are filtered by current `X-User-Id`;
- `detailState` is implementation-defined but must distinguish at least `ready`, `error`, and stale-cache scenarios;
- panel/NAT failures must not remove core machine fields returned from main-site sync.
