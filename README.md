# catnap

`lazycats.online/cart` 的库存监控与通知（同源 Web UI + Rust 后端）。

## Quick start（本地）

### 1) 准备

- Rust（稳定版）
- Bun（用于 `web/`）

### 2) 构建 Web

```bash
cd web
bun install
bun run build
```

### 3) 运行后端

> 说明：本项目默认要求由受信任的反向代理注入“用户标识”header，浏览器无法直接手动添加 header。

```bash
export CATNAP_AUTH_USER_HEADER=X-User-Id
export BIND_ADDR=127.0.0.1:18080

# 可选
export CATNAP_DB_URL=sqlite:catnap.db
export CATNAP_UPSTREAM_CART_URL=https://lazycats.online/cart

cargo run
```

### 4) 通过反向代理注入用户 header（示例：Caddy）

新建 `Caddyfile`：

```caddyfile
:8080 {
  reverse_proxy 127.0.0.1:18080 {
    header_up X-User-Id u_1
    header_up X-Forwarded-Proto http
  }
}
```

然后启动 Caddy（按你的安装方式启动即可），用浏览器访问：

- `http://127.0.0.1:8080/`

### 5) API 试跑（无需浏览器）

```bash
curl -sS \
  -H 'Host: 127.0.0.1:18080' \
  -H 'X-User-Id: u_1' \
  -H 'Origin: http://127.0.0.1:18080' \
  http://127.0.0.1:18080/api/bootstrap | jq
```

## 通知配置

### Telegram

在 UI 的「系统设置」里填写并启用：

- `bot token`
- `target`（chat id / 频道）

### Web Push（可选）

服务端需要提供 VAPID public key（base64url）：

```bash
export CATNAP_WEB_PUSH_VAPID_PUBLIC_KEY='...'
```

然后在 UI 的「系统设置」里勾选 Web Push，并点击「注册并上传订阅」。

> 浏览器 Push 通常要求 HTTPS（或 localhost）。

## 常用命令

后端：

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
```

前端：

```bash
cd web
bun run lint
bun run typecheck
bun run build
```

## Docker

```bash
docker build -t catnap .
docker run --rm -p 18080:18080 \
  -e CATNAP_AUTH_USER_HEADER=X-User-Id \
  -e CATNAP_DB_URL=sqlite:/app/catnap.db \
  catnap
```

> 仍建议通过反向代理注入 `X-User-Id` 并保持同源访问。

