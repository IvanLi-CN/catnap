# Codex：安装 UI UX Pro Max 技能（#xvecz）

## 状态

- Status: 待实现
- Created: 2026-02-26
- Last: 2026-02-26

## 背景 / 问题陈述

当前仓库尚未内置 Codex 的 UI/UX 能力。团队希望把 [UI UX Pro Max](https://ui-ux-pro-max-skill.nextlevelbuilder.io/#styles) 安装到项目级 `.codex`，并将安装产物纳入版本库，以便团队成员在同一仓库中获得一致的技能能力（尤其是 styles 相关检索与设计系统建议）。

## 目标 / 非目标

### Goals

- 在仓库根目录生成 `.codex/skills/ui-ux-pro-max/` 技能目录。
- 保持 `.codex` 默认忽略，仅放开 `.codex/skills/ui-ux-pro-max/**` 的 Git 追踪。
- 完成本地可执行验证（脚本 smoke test）并走 fast-track 到 PR/checks 结果明确。

### Non-goals

- 不修改业务代码（Rust API、Web UI、数据库 schema）。
- 不引入除 UI UX Pro Max 外的其它新技能。

## 范围（Scope）

### In scope

- 升级 `uipro-cli` 到最新稳定版本。
- 执行 `uipro init --ai codex` 完成项目级安装。
- 调整 `.gitignore` 的 `.codex` 追踪边界。
- 进行最小功能验证、提交、push、PR 与 checks 跟踪。

### Out of scope

- 全局 `~/.codex/skills` 安装。
- 额外 UI/UX 页面改造或样式实现。

## 需求（Requirements）

### MUST

- 生成并追踪以下关键文件：
  - `.codex/skills/ui-ux-pro-max/SKILL.md`
  - `.codex/skills/ui-ux-pro-max/scripts/search.py`
  - `.codex/skills/ui-ux-pro-max/data/styles.csv`
- `python3 .codex/skills/ui-ux-pro-max/scripts/search.py "saas minimal" --domain style -n 3` 可执行成功。
- 提交信息遵循 conventional commits，且使用 `--signoff`。

## 接口契约（Interfaces & Contracts）

None（不新增/修改/删除任何运行时接口或数据契约）。

## 验收标准（Acceptance Criteria）

- Given 仓库为干净工作区
  When 执行安装流程
  Then `.codex/skills/ui-ux-pro-max/**` 文件生成且可被 Git 正确追踪

- Given 安装完成
  When 执行 `search.py` style 域查询
  Then 返回结果且进程正常退出（非报错）

- Given 创建 PR 后触发 checks
  When CI 运行结束
  Then checks 状态明确（全绿或明确阻塞并给出处理结果）

## 非功能性验收 / 质量门槛（Quality Gates）

- 安装与 smoke test 输出可复现。
- PR 阶段状态字段完整：`PR状态`、`review阻塞项`、`自动修复轮次`、`越界问题`。

## 实现里程碑（Milestones）

- [ ] M1: 新增 spec（`docs/specs/xvecz-install-ui-ux-pro-max-codex/SPEC.md`）并登记 index
- [ ] M2: 完成 `uipro-cli` 升级与项目级 Codex 技能安装
- [ ] M3: 完成 `.gitignore` 边界调整与 smoke test
- [ ] M4: 完成提交、push、PR 创建与 checks/review-loop 收敛
