# catnap

`lazycats.vip/cart` 的库存监控与通知（同源 Web UI + Rust 后端）。

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
export CATNAP_UPSTREAM_CART_URL=https://lazycats.vip/cart

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

## Releases / Images / Versioning

## CI（PR gating）

为缩短 PR 反馈时延，CI 会按变更路径做 job/step gating（契约：`.github/scripts/ci-path-gate.sh`）：

- `Release Chain Smoke (PR)`：仅当命中以下任一变更时才会运行：`Dockerfile`、`.github/**`、`deploy/**`、`Cargo.toml`/`Cargo.lock`、`src/**`、`web/**`
- 前端重型检查（storybook + test-storybook + Playwright 安装）：仅当 `web/**` 变更时才会运行
- 注意：为了满足后端对 `web/dist` 的 embed 依赖，CI 仍会先执行一次 `bun run build` 来生成 `web/dist`

### Versioning（版本号）

- Tag：`v<semver>`（例如 `v0.1.0`）
- CI 会计算并注入 `APP_EFFECTIVE_VERSION=<semver>`（Docker env + `/api/health.version`）
- `/api/health`：
  - `status=ok`
  - `version=<semver>`

### Release intent（发版意图标签）

合并到 `main` 的 PR 必须且只能选择一个标签（CI 会强制）：

- `type:docs` / `type:skip`：不允许自动发版
- `type:patch` / `type:minor` / `type:major`：允许自动发版，并按标签 bump 版本号

> 无法关联到 PR 的 `push main`（direct push / 异常合并）默认跳过自动发版（仍会跑 lint/tests）。

### GHCR images（镜像）

- 镜像：`ghcr.io/<owner>/catnap`
- Tags（最低集合）：
  - `v<semver>`
  - `latest`（仅 `main` 自动发版会更新）

示例：

```bash
docker pull ghcr.io/<owner>/catnap:v0.1.0
docker pull ghcr.io/<owner>/catnap:latest
```

### GitHub Release assets（二进制）

每个 Release 会附带可复用的二进制 tarball + sha256（共 8 个文件）：

- `catnap_<semver>_linux_amd64_gnu.tar.gz`
- `catnap_<semver>_linux_arm64_gnu.tar.gz`
- `catnap_<semver>_linux_amd64_musl.tar.gz`
- `catnap_<semver>_linux_arm64_musl.tar.gz`
- 以及对应的 `.sha256`

校验示例：

```bash
sha256sum -c catnap_<semver>_linux_amd64_gnu.tar.gz.sha256
```

### Manual publish（workflow_dispatch）

在 GitHub Actions 手动触发 `workflow_dispatch`：

- ref=`main`：完整发布链路（tag/release/assets/GHCR；并更新 `latest`）；必须提供 `bump_level=major|minor|patch`
- ref=`refs/tags/v<semver>`：重跑/补齐该版本（assets/GHCR；不更新 `latest`）

### Smoke test（本地/CI）

```bash
APP_EFFECTIVE_VERSION=0.1.0 bash ./.github/scripts/smoke-test.sh ./target/release/catnap
```
