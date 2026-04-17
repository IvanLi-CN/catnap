## GET /api/lazycat/machines

`items[]` 在既有字段基础上新增：

```json
{
  "serviceId": 2312,
  "serviceName": "港湾 Transit Mini",
  "panelUrl": "https://edge-node-24.example.net:8443/container/dashboard?hash=8d1f0c27b4a9e3f2",
  "detailUrl": "https://lxc.lazycat.wiki/servicedetail?id=2312"
}
```

Rules:

- `detailUrl` 为可选字段：当 `CATNAP_LAZYCAT_BASE_URL` 不是合法 `http(s)` URL 时允许省略。
- `detailUrl` 必须由服务端基于 `CATNAP_LAZYCAT_BASE_URL` 归一化生成，路径固定为 `/servicedetail?id=<serviceId>`。
- 前端只消费返回字段，不得从 `panelUrl`、`catalog.source.url`、`settings.siteBaseUrl` 或示例常量推导详情页链接。

## POST /api/lazycat/machines/:service_id/detail-login-bridge

- Auth: same as existing Catnap API user identity.
- Purpose: provide one-shot popup auto-login material for the real upstream lazycat detail page.
- Behavior:
  - requires a same-origin `POST` submission from the current Catnap page;
  - returns a JSON payload that the front-end uses to submit the saved lazycat credentials into the already-open popup;
  - every click fetches a fresh login token from upstream `/login`;
  - response must be marked `Cache-Control: no-store`.

### Success responses

- Status: `200 OK`
  - Body:

```json
{
  "loginUrl": "https://lxc.lazycat.wiki/login?action=email",
  "targetUrl": "https://lxc.lazycat.wiki/servicedetail?id=2312",
  "email": "first@example.com",
  "password": "secret",
  "token": "bridge-token-2312",
  "primeDelayMs": 1500,
  "redirectAfterMs": 8000
}
```

Rules:

- front-end must first open `targetUrl` in a popup, then submit the login form with `target=<same popup name>`;
- after `primeDelayMs + redirectAfterMs`, front-end must perform one best-effort popup re-navigation back to `targetUrl`;
- Catnap must not return upstream detail HTML as a same-origin proxy page.

### Failure response

- Status: `400`
- Body: JSON error payload explaining that the upstream detail page auto-login material could not be prepared.
