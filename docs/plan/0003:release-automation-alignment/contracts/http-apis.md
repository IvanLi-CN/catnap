# HTTP APIs Contracts（#0003）

本文件定义发版与 smoke test 相关的 HTTP 契约（health + 静态 UI 路由）。

## `GET /api/health`

- Scope: external
- Change: Modify
- Auth: none

### Response (200)

Content-Type: `application/json`

Body:

```json
{
  "status": "ok",
  "version": "0.1.0"
}
```

Fields:

- `status` (string, required): 固定为 `ok`
- `version` (string, required): 运行中服务的版本号（与发布版本一致，形如 `x.y.z`）

## Static UI routes

静态 UI 必须由同一服务端进程提供，并且静态资源必须 embed 到二进制内（不依赖运行时文件系统目录）。

### `GET /`

- Scope: external
- Change: Modify
- Auth: none
- Response (200):
  - Content-Type: `text/html`（或等价）
  - Body: `index.html`

### `GET /assets/*`

- Scope: external
- Change: Modify
- Auth: none
- Response:
  - `200`: 返回静态资源（`application/javascript` / `text/css` / fonts 等）
  - `404`: 资源不存在
