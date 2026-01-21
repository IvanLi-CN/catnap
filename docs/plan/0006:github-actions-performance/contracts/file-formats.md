# 文件格式（File formats）

将 CI 的“触发规则、缓存语义、产物布局”视为一种接口契约来描述。

## CI gating rules（paths/labels → jobs）

- 范围（Scope）: internal
- 变更（Change）: New
- 编码（Encoding）: utf-8（YAML + 规则文本）

### Schema（结构）

- gating 信号来源：
  - `ci-path-gate.sh` 输出的 `*_changed` outputs（推荐），或等价实现（需保持同等语义）。
- 至少需要冻结以下规则（最终以 `../PLAN.md` 的验收为准）：
  - 何时运行 `Release Chain Smoke (PR)`
  - 何时运行前端重型检查（storybook/test-storybook/playwright install）
  - PR 阶段 docker build platforms（冻结：仅 `linux/amd64`）

### Examples（示例）

- `frontend_changed=true` → 运行前端重型检查
- `docker_changed=true` → 运行 `Release Chain Smoke (PR)`（并按策略决定 platforms）

### Frozen rules（已冻结规则）

- 前端重型检查（storybook/test-storybook/playwright install）：
  - 触发：`web/**` 发生变更
- `Release Chain Smoke (PR)`：
  - 触发：任一命中则运行
    - `Dockerfile`
    - `.github/**`
    - `Cargo.toml`、`Cargo.lock`
    - `src/**`
    - `web/**`
    - `deploy/**`
  - 否则：可跳过（例如 `docs/**` 变更）
- PR docker build platforms：
  - 固定：仅 `linux/amd64`（PR 阶段跳过 `linux/arm64`）

### 兼容性与迁移（Compatibility / migration）

- gating 规则属于覆盖面变更：必须可回滚到“全量跑”（无 gating）形态。

## Docker build cache semantics（Buildx cache）

- 范围（Scope）: internal
- 变更（Change）: Modify
- 编码（Encoding）: n/a（YAML inputs）

### Schema（结构）

- Docker build cache backend 冻结为：`type=gha`（GitHub Actions cache backend）
- cache key 必须隐含或显式绑定到：
  - `Dockerfile`
  - `Cargo.lock` / `Cargo.toml`
  - `web/bun.lock*` / `web/package.json`
- cache export 失败语义（best-effort）：
  - “仅 cache 写入失败”不得让 build 失败；
  - 真实 build 失败必须 fail；
  - 日志需输出 warning（用于排障与观察频率）。

### Examples（示例）

- Buildx（示意，不代表最终实现）：

```yaml
cache-from: type=gha
cache-to: type=gha,mode=max
```

### 兼容性与迁移（Compatibility / migration）

- backend 切换属于接口变更：如需从 `type=gha` 切换到其他 backend，必须更新本契约并保证可回滚。
