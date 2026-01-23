# Config（#0008）

## Web Push：VAPID keys

现状：服务端仅支持配置 `CATNAP_WEB_PUSH_VAPID_PUBLIC_KEY`（用于浏览器订阅时的 `applicationServerKey`），但缺少“发送 push”所需的 private key。

本计划拟新增/补齐以下配置（internal）：

- `CATNAP_WEB_PUSH_VAPID_PUBLIC_KEY`（已存在）：VAPID public key（base64url）
- `CATNAP_WEB_PUSH_VAPID_PRIVATE_KEY`（新增）：VAPID private key（base64url）
- `CATNAP_WEB_PUSH_VAPID_SUBJECT`（新增）：VAPID subject（建议 `mailto:...` 或站点 URL）

### 行为约定

- 若缺少 `CATNAP_WEB_PUSH_VAPID_PUBLIC_KEY`：UI 侧无法完成订阅（现有行为保持）。
- 若缺少 `CATNAP_WEB_PUSH_VAPID_PRIVATE_KEY` 或 `CATNAP_WEB_PUSH_VAPID_SUBJECT`：允许 UI 展示 Web Push 配置，但 `POST /api/notifications/web-push/test` 必须失败并给出可行动的错误信息（不泄漏敏感信息）。

