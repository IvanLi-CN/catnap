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
