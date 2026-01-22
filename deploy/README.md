# 部署（Docker / Compose）

本目录提供一套可直接落地的 Docker 部署方案：`catnap` 服务本体 + `Caddy` 反向代理（用于同源访问与注入用户标识 header）。

> 重要：当前 `catnap` 的默认鉴权模型是假设“由受信任的反向代理注入用户标识 header”。如果你要把服务暴露到公网，请先在反向代理层做强鉴权（例如 Basic/OIDC/网关），不要把固定的用户 ID header 暴露给不受信任的客户端。

## 前置条件

- Docker + Docker Compose（优先 `docker compose`；也可用 `docker-compose`）

## Quick start（本机一键起）

1) 在 `deploy/` 下创建环境变量文件：

```bash
cp deploy/.env.example deploy/.env
```

2) 启动（默认会从本仓库源码构建镜像）：

```bash
cd deploy

docker compose up -d --build

# Docker Compose v1
# docker-compose up -d --build
```

3) 浏览器访问（同源）：

- `http://127.0.0.1:8080/`

## 目录说明

- `deploy/compose.yaml`：`catnap` + `caddy` 的 compose 定义（含 SQLite 持久化 volume）。
- `deploy/Caddyfile`：反向代理配置（示例会注入 `X-User-Id: u_1`）。
- `deploy/.env.example`：可复制为 `deploy/.env` 的环境变量模板。

注意：`CATNAP_AUTH_USER_HEADER` 需要与 `deploy/Caddyfile` 里 `header_up ...` 的 header 名保持一致。

## 数据持久化（SQLite）

默认将 SQLite 文件放到 named volume：`catnap-data:/data`，并通过 `CATNAP_DB_URL=sqlite:/data/catnap.db` 指向它。

备份（导出 volume 内容到当前目录）示例：

```bash
docker run --rm \
  -v catnap-data:/data:ro \
  -v "$PWD":/backup \
  busybox \
  sh -c 'tar -czf /backup/catnap-data.tgz -C /data .'
```

恢复示例（会覆盖 volume 内现有内容）：

```bash
docker run --rm \
  -v catnap-data:/data \
  -v "$PWD":/backup \
  busybox \
  sh -c 'rm -rf /data/* && tar -xzf /backup/catnap-data.tgz -C /data'
```

## 使用 GHCR 镜像（可选）

如果你不想从源码构建，可以在 `deploy/.env` 里设置：

```bash
CATNAP_IMAGE=ghcr.io/<owner>/catnap:latest
```

然后执行：

```bash
cd deploy

docker compose pull
docker compose up -d

# Docker Compose v1
# docker-compose pull
# docker-compose up -d
```

## 生产化提示（需要你做选择）

- 域名与 HTTPS：把 `deploy/Caddyfile` 的 `:8080` 换成你的域名，并在 compose 中映射 80/443。
- 多用户：把“注入固定用户 ID”的做法替换为真正的鉴权与身份映射（网关/OIDC/SSO 等）。
