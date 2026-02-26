# Config Contracts（#wzc6m）

本文件定义 “关于/更新提示” 功能使用的新增环境变量。

## Repo meta

- `CATNAP_REPO_URL` (optional)
  - Default: `https://github.com/IvanLi-CN/catnap`
  - Purpose: UI 展示仓库地址（fork/私有部署可覆盖）

## Update-check（GitHub Releases stable latest）

- `CATNAP_UPDATE_REPO` (optional)
  - Default: `IvanLi-CN/catnap`
  - Format: `<owner>/<repo>`

- `CATNAP_UPDATE_CHECK_ENABLED` (optional)
  - Default: `true`
  - Values: `true|false`

- `CATNAP_UPDATE_CHECK_TTL_SECONDS` (optional)
  - Default: `21600` (6h)
  - Purpose: 缓存 TTL，避免频繁外网请求

- `CATNAP_UPDATE_CHECK_TIMEOUT_MS` (optional)
  - Default: `1500`
  - Purpose: 外网请求超时（上限），失败不影响主功能

- `CATNAP_GITHUB_API_BASE_URL` (optional)
  - Default: `https://api.github.com`
  - Purpose: 测试/自建 GitHub Enterprise 的 override

