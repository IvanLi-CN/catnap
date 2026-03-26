# catnap

`lxc.lazycat.wiki/cart` 的库存监控与通知（同源 Web UI + Rust 后端）。

## 功能

- 监控 `lxc.lazycat.wiki/cart` 的库存变化
- Telegram 通知
- Web Push（可选）
- 采集观测台（`#ops`：全局队列/worker/成功率/cache hit/目录拓扑状态 + SSE 日志 tail）
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
export CATNAP_UPSTREAM_CART_URL=https://lxc.lazycat.wiki/cart

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
- `http://127.0.0.1:8080/#ops`

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
- `CATNAP_UPSTREAM_CART_URL`：上游页面，默认 `https://lxc.lazycat.wiki/cart`（注意：`/cart` 不要带尾随 `/`，例如 `/cart/` 可能 404）
- `CATNAP_TELEGRAM_API_BASE_URL`：Telegram Bot API base URL（默认 `https://api.telegram.org`；用于测试 stub）
- `CATNAP_WEB_PUSH_VAPID_PUBLIC_KEY`：Web Push VAPID public key（base64url，可选）
- `CATNAP_WEB_PUSH_VAPID_PRIVATE_KEY`：Web Push VAPID private key（base64url，可选；用于服务端发送测试 Push）
- `CATNAP_WEB_PUSH_VAPID_SUBJECT`：Web Push VAPID subject（建议 `mailto:` 或站点 URL；用于服务端发送测试 Push）
- `CATNAP_DEFAULT_POLL_INTERVAL_MINUTES`：默认轮询间隔（分钟，>= 1），默认 `1`
- `CATNAP_DEFAULT_POLL_JITTER_PCT`：默认抖动比例（0..=1），默认 `0.1`
- `CATNAP_LOG_RETENTION_DAYS`：日志保留天数（>= 0），默认 `7`
- `CATNAP_LOG_RETENTION_MAX_ROWS`：日志最大行数（>= 0），默认 `10000`
- `CATNAP_OPS_WORKER_CONCURRENCY`：采集 worker 并发数（>= 1），默认 `2`
- 启动后优先使用本地 DB catalog；目录拓扑（root/fid）按低频复扫，已知 `url_key` 页面由 discovery/poller 渐进轻扫
- `CATNAP_OPS_SSE_REPLAY_WINDOW_SECONDS`：ops SSE 回放窗口（秒，>= 1），默认 `3600`
- `CATNAP_OPS_LOG_RETENTION_DAYS`：ops 事件/运行记录保留天数（>= 0），默认 `7`
- `CATNAP_OPS_LOG_TAIL_LIMIT_DEFAULT`：`/api/ops/state` 默认 `logLimit`，默认 `200`
- `CATNAP_OPS_QUEUE_TASK_LIMIT_DEFAULT`：`/api/ops/state` 默认 `taskLimit`，默认 `200`

## 通知配置

### Telegram

在 UI 的「系统设置」里填写并启用：

- `bot token`
- `targets`（可添加多个 chat id / 群 / 频道）

保存后可点击「测试 Telegram」立即验证全部目标是否可用；测试请求会返回逐目标结果，且不会在 API 响应或日志中泄漏 token 明文。

当前默认通知文案示例：

```text
【补货 + 价格变动】芬兰特惠年付 Mini
库存 0 → 3｜价格 ¥4.99 → ¥3.99 / 年
查看监控：https://catnap.example/monitoring
```

```text
【Telegram 测试】通知配置正常
如果你看到这条消息，说明 Catnap 已可发送 Telegram 通知。
时间：2026-03-06 15:00:00Z
```

说明：用户通知默认不展示 raw `lc:*` 配置 ID；机器可读的技术文案仍保留在日志中。

常见排障建议：

- `chat not found`：先确认对应 `target` 是否正确（频道用户名用 `@channelusername`，群/超级群通常是数字 id）。
- 若错误里包含 `migrate_to_chat_id=<...>`：说明群已迁移到超级群，直接把该 `target` 改为返回值（通常以 `-100` 开头）。
- 若报权限相关错误：确认 bot 已被拉入目标群/频道，并具备发送消息权限（频道通常需要管理员权限）。

### Web Push（可选）

服务端需要提供 VAPID public key（base64url）：

```bash
export CATNAP_WEB_PUSH_VAPID_PUBLIC_KEY='...'
```

然后在 UI 的「系统设置」里勾选 Web Push，并点击「启用推送」（请求权限 → 注册 Service Worker → 上传 subscription）。

若要使用「测试 Web Push」，服务端还需要：

```bash
export CATNAP_WEB_PUSH_VAPID_PRIVATE_KEY='...'
export CATNAP_WEB_PUSH_VAPID_SUBJECT='mailto:you@example.com'
```

> 浏览器 Push 通常要求 HTTPS（或 localhost）。

当前默认 Web Push 文案示例：

- title: `Catnap · 新上架`
- body: `芬兰特惠年付 Mini｜库存 5｜¥4.99 / 年`
- test title/body: `Catnap · 测试通知` / `Web Push 已连通，点击返回设置页。`

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

# Vite dev (fixed port: 18182; avoid default 5173 to reduce conflicts)
bun run dev

# Storybook (fixed port: 18181; if you see 6006 you likely didn't use the script or ran from the wrong cwd)
bun run storybook
bun run storybook:ci
```

## 部署

### Docker（单容器）

```bash
docker build -t catnap .
docker run --rm -p 18080:18080 \
  -e CATNAP_AUTH_USER_HEADER=X-User-Id \
  -e CATNAP_DB_URL=sqlite:///app/catnap.db \
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

- stable tag：`v<semver>`（例如 `v0.10.0`）
- rc tag：`v<semver>-rc.<sha7>`（例如 `v0.11.0-rc.1a2b3c4`）
- `APP_EFFECTIVE_VERSION` 由 immutable release snapshot 冻结，并注入 Docker env + `/api/health.version`
- `/api/health`：
  - `status=ok`
  - `version=<APP_EFFECTIVE_VERSION>`

### Release labels（发版标签）

合并到 `main` 的 PR 必须同时且只能拥有以下两类标签（`PR Label Gate` 会强制）：

- 一个 `type:*`：`type:docs` / `type:skip` / `type:patch` / `type:minor` / `type:major`
- 一个 `channel:*`：`channel:stable` / `channel:rc`

语义：

- `type:docs` / `type:skip`：会生成 snapshot，但不会正式发版
- `type:patch` / `type:minor` / `type:major` + `channel:stable`：发布 stable 版本
- `type:patch` / `type:minor` / `type:major` + `channel:rc`：发布 rc 版本，tag 带 `-rc.<sha7>` 后缀

### Release workflows（发布工作流）

发布链路已经拆分为三段：

- `CI PR`：PR / merge queue 校验，只负责 required checks
- `CI Main`：`push main` 后固化 immutable release snapshot，不直接发布
- `Release`：只消费 snapshot 发版，并自动 drain pending backlog

snapshot 使用 git notes `refs/notes/release-snapshots` 保存 `target_sha`、PR 编号、标签、版本号、release tag 与镜像 tags。发布阶段不会再重新猜测 bump level 或关联 PR。

如果需要对“曾修改 `.github/workflows/**` 的历史 commit”做补发，仓库还需要配置 `RELEASE_WORKFLOW_TOKEN` secret。原因是默认 `GITHUB_TOKEN` 无法为这类 commit 创建 tag / GitHub Release；该 override token 必须具备创建 release、写 PR 评论，以及处理 workflow commit 所需的权限。

### GHCR images（镜像）

- 镜像：`ghcr.io/<owner>/catnap`
- Tags（最低集合）：
  - `v<semver>` 或 `v<semver>-rc.<sha7>`
  - `latest`：仅在当前 stable snapshot 没有被**更高版本的已发布 stable snapshot**超车时发布；未发布的 pending stable snapshot 不得提前压掉 `latest`
- 发布链路会复用已产出的 linux gnu release binaries 来封装 GHCR 镜像，避免在多架构 Docker build 中重复编译 Rust。

示例：

```bash
docker pull ghcr.io/<owner>/catnap:v0.10.0
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

### Manual backfill（workflow_dispatch）

在 GitHub Actions 手动触发 `Release` workflow 的 `workflow_dispatch`：

- 只接受 `commit_sha=<main 上的 commit SHA>`
- 用途：补发漏掉的版本，或重跑已冻结 snapshot 对应的发布链路
- 手动补发会先校验该 commit 曾经通过 `CI Main`，或在迁移期通过过旧 `CI Pipeline` 的 `push main` 校验
- 历史上没有 `channel:*` 的已合并未发布 PR，仅在这条手动 backfill 路径上允许一次性默认映射为 `channel:stable`
- 若目标 commit 修改过 `.github/workflows/**`，还必须先配置 `RELEASE_WORKFLOW_TOKEN`，否则 GitHub 会拒绝为该 commit 创建 release/tag

旧入口已经退役：

- 不再使用 `workflow_dispatch ref=main + bump_level=*`
- 不再使用 tag 输入型 `release-backfill.yml`

### PR release comment（PR 版本评论）

发布成功后，`Release` 会自动在对应 PR 的 timeline upsert 一条版本评论，内容包含：

- release tag
- `APP_EFFECTIVE_VERSION`
- channel
- target commit
- GitHub Release 链接

### Smoke test（本地/CI）

```bash
APP_EFFECTIVE_VERSION=0.1.0 bash ./.github/scripts/smoke-test.sh ./target/release/catnap
```
