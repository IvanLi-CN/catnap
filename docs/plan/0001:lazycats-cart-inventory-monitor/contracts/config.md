# Runtime config（环境变量）

本文件描述运行时配置项（env vars）及默认值。**这些配置仅用于服务端与运维，不应在 UI/API 错误信息中泄露实现细节。**

> 注：变量名在计划阶段先冻结口径；实现阶段按此落地。

## 服务运行

- `BIND_ADDR`（optional）
  - 说明：监听地址
  - 默认：`0.0.0.0:18080`
  - 示例：`127.0.0.1:18080`

- `STATIC_DIR`（optional）
  - 说明：静态资源目录（Web build 输出）
  - 默认：`web/dist`

- `APP_EFFECTIVE_VERSION`（optional）
  - 说明：部署版本（CI/容器注入）
  - 默认：`Cargo.toml` 的 `version`

## 上游抓取（lazycats）

- `CATNAP_UPSTREAM_CART_URL`（optional）
  - 说明：上游入口 URL（用于抓取国家地区/区域/配置）
  - 默认：`https://lazycats.online/cart`

## 鉴权（不对客户端暴露细节）

- `CATNAP_AUTH_USER_HEADER`（required）
  - 说明：受信任上游注入“用户标识”的 header 名称（例如反向代理注入）。
  - 行为：若变量为空或请求缺少该 header，则返回 401（Web：401 页面；API：JSON 401），且错误信息不包含任何鉴权细节。

## 轮询默认值

- `CATNAP_DEFAULT_POLL_INTERVAL_MINUTES`（optional）
  - 说明：默认查询频率（分钟）
  - 默认：`1`

- `CATNAP_DEFAULT_POLL_JITTER_PCT`（optional）
  - 说明：默认抖动比例（0..1）
  - 默认：`0.1`（即 0–10% 随机延迟）

## 日志保留

- `CATNAP_LOG_RETENTION_DAYS`（optional）
  - 说明：日志保留天数
  - 默认：`7`

- `CATNAP_LOG_RETENTION_MAX_ROWS`（optional）
  - 说明：日志最多保留条数
  - 默认：`10000`

## Web Push（VAPID）

- `CATNAP_WEB_PUSH_VAPID_PUBLIC_KEY`（required, if web push enabled）
  - 说明：VAPID public key（供浏览器订阅使用）

- `CATNAP_WEB_PUSH_VAPID_PRIVATE_KEY`（required, if web push enabled）
  - 说明：VAPID private key（仅服务端使用）

- `CATNAP_WEB_PUSH_SUBJECT`（optional）
  - 说明：VAPID subject（mailto 或 https URL）
  - 默认：`mailto:admin@localhost`

## 持久化

- `CATNAP_DB_URL`（optional）
  - 说明：SQLite 连接字符串或路径
  - 默认：`sqlite:catnap.db`
