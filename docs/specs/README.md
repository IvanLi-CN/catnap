# 规格（Spec）总览

本目录用于管理工作项的**规格与追踪**：记录范围、验收标准、任务清单与状态，作为交付依据；实现与验证应以对应 `SPEC.md` 为准。

> Legacy compatibility: 历史规格仍保留在 `docs/plan/**/PLAN.md`（只读兼容）。新规格统一创建在 `docs/specs/**/SPEC.md`。

## 目录与命名规则

- 每个规格一个目录：`docs/specs/<id>-<title>/`
- `<id>`：推荐 5 个字符 nanoId 风格（字符集：`23456789abcdefghjkmnpqrstuvwxyz`）
- `<title>`：简短、稳定的 kebab-case slug

## 状态（Status）说明

仅允许使用以下状态值：

- `待设计`
- `待实现`
- `跳过`
- `部分完成（x/y）`
- `已完成`
- `作废`
- `重新设计（#<id>）`

## Index

| ID   | Title | Status | Spec | Last | Notes |
|-----:|-------|--------|------|------|-------|
| pgnnw | 发布链路修复与 GHCR 回填闭环（Dockrev 无候选） | 部分完成（4/5） | `pgnnw-release-ghcr-chain-fix/SPEC.md` | 2026-02-25 | fast-track |
