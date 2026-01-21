# CLI Contracts（#0003）

本文件定义“发版链路”中被 CI 调用的脚本接口（面向 CI/维护者），用于稳定地产出版本号口径，并对构建产物做最小运行验证。

## `.github/scripts/compute-version.sh`

- Scope: internal
- Change: Modify

### Purpose

计算本次发布的有效版本号，并导出到环境变量 `APP_EFFECTIVE_VERSION`。

### Inputs

- `Cargo.toml` 中的 `version`（形如 `x.y.z`）
- 已存在的 git tags（形如 `v<semver>`）

### Outputs

- 向 `$GITHUB_ENV` 写入：`APP_EFFECTIVE_VERSION=<semver>`
- stdout 打印 computed version（用于日志）

### Exit codes

- `0`: 计算成功
- `!= 0`: 计算失败（例如无法读取 `Cargo.toml` 版本号）

## `.github/scripts/smoke-test.sh` (planned)

- Scope: internal
- Change: New

### Purpose

在 CI 中对“构建产物”做最小运行验证（启动服务端→探活→访问 UI）。

### Inputs

- `CATNAP_SMOKE_BIN` (required): 要测试的二进制路径（例如 `./target/release/catnap`）
- `CATNAP_SMOKE_ADDR` (optional, default `127.0.0.1:18080`): 服务监听地址
- `CATNAP_SMOKE_TIMEOUT_SECONDS` (optional, default `20`): 等待就绪超时时间
- `APP_EFFECTIVE_VERSION` (optional): 若提供，则校验 `/api/health` 返回的 `version` 与其一致

### Behavior

- 启动 `catnap` 并等待就绪
- 必须通过（最小集合）：
  - `GET /api/health` → `200` 且 `status=ok`
  - `GET /` → `200` 且返回 HTML（可用 `<!doctype html>` 作为最小断言）
  - （可选）若提供 `APP_EFFECTIVE_VERSION`：`/api/health.version` 必须与其一致
- 结束时必须清理子进程（即使失败也要退出前 kill）

### Exit codes

- `0`: smoke test 通过
- `!= 0`: smoke test 失败（启动失败/超时/端点不符合契约）
