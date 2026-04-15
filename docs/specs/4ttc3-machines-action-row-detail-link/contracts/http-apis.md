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

## POST /api/lazycat/machines/:service_id/detail-bridge

- Auth: same as existing Catnap API user identity.
- Purpose: open the upstream lazycat detail page in a new window while keeping the Catnap main UI on the same origin.
- Behavior:
  - requires a same-origin `POST` submission from the current Catnap page;
  - returns a short-lived HTML bridge that auto-submits the saved lazycat credentials and then lands on the real `detailUrl`;
  - if the upstream login page establishes a browser-bound anonymous session first, the bridge must first prime the browser with a real `GET /login` in the popup target before replaying the saved login form, so the browser receives any anonymous session / CSRF cookie needed by the subsequent login POST.

### Success responses

- Status: `200 OK`
  - Body: HTML bridge page that primes upstream login when needed, auto-submits the lazycat login form, and then navigates to the real upstream detail page.

### Failure response

- Status: `400` or `502`
- Body: HTML error page explaining that the upstream detail page could not be opened.
