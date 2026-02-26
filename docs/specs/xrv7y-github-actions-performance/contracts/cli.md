# 命令行（CLI）

本文件定义“CI gating”相关脚本的接口契约（面向 CI/维护者），用于在不引入额外第三方 Action 的前提下实现 job gating。

## `.github/scripts/ci-path-gate.sh`

- 范围（Scope）: internal
- 变更（Change）: New

### 用法（Usage）

```text
bash ./.github/scripts/ci-path-gate.sh >> "$GITHUB_OUTPUT"
```

### 输入（Inputs）

- 环境变量（Environment variables）：
  - `CI_BASE_SHA`：用于比较的 base commit（默认：PR base sha；若不可得则回退到 `origin/main` 的 merge-base）
  - `CI_HEAD_SHA`：用于比较的 head commit（默认：PR head sha；若不可得则回退到 `HEAD`）
  - `CI_ASSUME_CHANGED`：当无法获取 diff（例如浅克隆/缺失对象）时的兜底策略（`true|false`，默认 `true`）

### 输出（Output）

输出写入 `$GITHUB_OUTPUT`（GitHub Actions outputs），至少包含：

- `frontend_changed`: `true|false`（是否命中前端触发路径，如 `web/**`）
- `backend_changed`: `true|false`（是否命中后端触发路径，如 `src/**` / `Cargo.*`）
- `docker_changed`: `true|false`（是否命中 docker/release 相关路径，如 `Dockerfile` / `.github/**` / `deploy/**`）
- `reason`: `string`（用于排障：命中的规则/兜底原因）

### 退出码（Exit codes）

- `0`: 成功
- `>0`: 脚本自身错误（例如参数解析错误、git 无法运行等）

### 兼容性与迁移（Compatibility / migration）

- 该脚本属于内部接口，但其输出 keys 需要稳定：新增 keys 允许（向后兼容），删除/改名必须同步更新 workflow 并在本契约中记录。
