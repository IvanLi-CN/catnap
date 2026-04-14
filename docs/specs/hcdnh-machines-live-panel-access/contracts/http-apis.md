# HTTP APIs

## `POST /api/lazycat/machines/:service_id/panel-url`

- Auth: same as existing Catnap API user identity.
- Purpose: resolve the current live container panel URL for a machine.
- Behavior:
  - reads the saved lazycat account for the current user;
  - reuses saved cookies when possible;
  - if the upstream info page falls back to the login form, re-authenticates with the saved email/password;
  - refreshes saved cookies / `lastAuthenticatedAt` when the reauth session changed;
  - returns the current dashboard URL including the live `hash` query when resolution succeeds.

### Success response

```json
{
  "url": "https://edge-node-24.example.net:8443/container/dashboard?hash=live-hash-2312",
  "kind": "panel"
}
```

### Failure cases

- `400 INVALID_ARGUMENT`: user missing, no connected lazycat account, target machine missing, or machine has no container panel.
- `500 INTERNAL_ERROR`: unexpected internal failure while reading local state.

## `POST /api/lazycat/machines/:service_id/panel`

- Auth: same as existing Catnap API user identity.
- Purpose: open the live container panel in a new window without exposing the reauth flow to the main UI thread.
- Behavior:
  - calls the same live panel resolution flow as `panel-url`;
  - returns `303 See Other` with `Location=<resolved live panel url>` on success;
  - returns a rendered error page on failure so the popup/tab can show a user-readable reason.

### Success response

- Status: `303 See Other`
- Headers:
  - `Location: <live panel url>`
  - `Cache-Control: no-store`

### Failure response

- Status: `400` or `502`
- Body: HTML error page explaining that the Web panel redirect failed.
