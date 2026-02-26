# 命令行（CLI）

本文件定义 `compute-version.sh` 的 machine-readable 输出能力，以便在单个 workflow step 内安全读取版本号。

## `.github/scripts/compute-version.sh`

- 范围（Scope）: internal
- 变更（Change）: Modify

### 用法（Usage）

新增一个 machine-readable 模式（推荐选其一，最终实现必须与本契约一致）：

#### 方案 A：`--print-version`

```text
bash ./.github/scripts/compute-version.sh --print-version
```

- stdout：仅输出 `<semver>`（例如 `0.1.8`），不得包含额外日志。
- stderr：允许输出人类可读日志（可选）。

#### 方案 B：`--github-output`

```text
bash ./.github/scripts/compute-version.sh --github-output "$GITHUB_OUTPUT"
```

- 向目标文件写入 `APP_EFFECTIVE_VERSION=<semver>`（或 `version=<semver>`，需与调用方一致）。

### 兼容性与迁移（Compatibility / migration）

- 保持默认行为向后兼容（不带新参数时仍按既有方式输出日志/写入 `$GITHUB_ENV`）。
- machine-readable 模式的输出格式必须稳定；若变更，必须同步更新 `.github/workflows/ci.yml` 并更新本契约。
