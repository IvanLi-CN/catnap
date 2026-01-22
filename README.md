# catnap

`lazycats.vip/cart` 的库存监控与通知（同源 Web UI + Rust 后端）。

## 功能

- 监控 `lazycats.vip/cart` 的库存变化
- Telegram 通知
- Web Push（可选）
- SQLite 持久化（默认）

## 架构与访问模型

- 后端：Rust 服务（同时提供 API + 静态 Web UI）
- 前端：`web/` 构建产物 `web/dist` 会被后端 embed
- 访问模型：默认假设**受信任的反向代理**会注入“用户标识”header（浏览器无法手动添加该 header）

> 如果要对公网提供服务，请先在反向代理层增加强鉴权（例如 Basic/OIDC/网关），不要把固定用户 ID header 暴露给不受信任的客户端。

## 快速开始（本地开发）

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

## 配置（环境变量）

服务端配置均通过环境变量读取（见 `src/config.rs`）。常用项：

- `BIND_ADDR`：监听地址，默认 `0.0.0.0:18080`
- `CATNAP_AUTH_USER_HEADER`：用户标识 header 名（由反向代理注入），默认空（不启用）
- `CATNAP_DB_URL`：数据库连接串，默认 `sqlite:catnap.db`
- `CATNAP_UPSTREAM_CART_URL`：上游页面，默认 `https://lazycats.vip/cart`
- `CATNAP_WEB_PUSH_VAPID_PUBLIC_KEY`：Web Push VAPID public key（base64url，可选）
- `CATNAP_DEFAULT_POLL_INTERVAL_MINUTES`：默认轮询间隔（分钟，>= 1），默认 `1`
- `CATNAP_DEFAULT_POLL_JITTER_PCT`：默认抖动比例（0..=1），默认 `0.1`
- `CATNAP_LOG_RETENTION_DAYS`：日志保留天数（>= 0），默认 `7`
- `CATNAP_LOG_RETENTION_MAX_ROWS`：日志最大行数（>= 0），默认 `10000`

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

## 部署

### Docker（单容器）

```bash
docker build -t catnap .
docker run --rm -p 18080:18080 \
  -e CATNAP_AUTH_USER_HEADER=X-User-Id \
  -e CATNAP_DB_URL=sqlite:/app/catnap.db \
  catnap
```

> 仍建议通过反向代理注入 `X-User-Id` 并保持同源访问。

### Docker Compose（推荐）

使用现成的 compose + Caddy 反向代理示例：`deploy/`（包含 SQLite 持久化 volume + 同源反向代理注入 header）。

```bash
cp deploy/.env.example deploy/.env
cd deploy
docker compose up -d --build
# Docker Compose v1（如果你的环境没有 `docker compose` 子命令）
# docker-compose up -d --build
```

详情见 `deploy/README.md`。

## CI / Release

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
