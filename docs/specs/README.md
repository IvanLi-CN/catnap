# 规格（Spec）总览

本目录用于管理工作项的**规格与追踪**：记录范围、验收标准、任务清单与状态，作为交付依据；实现与验证应以对应 `SPEC.md` 为准。

> 本目录为项目内规格文档的唯一索引入口。

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
| xm4p2 | 通知记录页与 Telegram 深链 | 已完成 | `xm4p2-notification-records-telegram-deeplink/SPEC.md` | 2026-03-11 | PR #66：通知记录持久化、独立页面、TG 深链、无限滚动与 review-loop 收口完成 |
| 32dfj | 分区级监控与双上新通知 | 已完成 | `32dfj-partition-monitoring-new-machine-alerts/SPEC.md` | 2026-03-10 | PR #63：checks 全绿；review-loop 无阻塞项 |
| z7myy | CI/CD：补齐漏发版本并修复 squash merge 自动发版识别 | 部分完成（5/6） | `z7myy-release-intent-squash-fallback-backfill/SPEC.md` | 2026-03-09 | `v0.6.0` 已由 run `22850192479` 补发；PR #62 待 checks 通过并合并 |
| cnduu | 低压优先的上架发现优化 | 已完成 | `cnduu-low-pressure-discovery-refresh/SPEC.md` | 2026-03-08 | 已交付：DB-first 启动、topology refresh、discovery_due、cache-hit 复用与 ops/UI 可观测 |
| z9x5g | 通知文案优化：简洁告警风格 | 已完成 | `z9x5g-notification-copy-optimization/SPEC.md` | 2026-03-07 | PR #59：通知文案 builder、测试覆盖与 README 示例已完成；待 checks 最终结果 |
| vycru | 设置页通知测试成功气泡化 | 已完成 | `vycru-settings-test-success-bubbles/SPEC.md` | 2026-03-07 | 统一 success/error feedback bubble、Storybook Docs/Stories 与视觉证据已完成 |
| zuhzt | 补齐可用区域说明并替换无效分组文案 | 已完成 | `zuhzt-region-notice-sync/SPEC.md` | 2026-03-05 | PR #55：区域说明链路上线 + review-loop 收敛完成 |
| pc6du | 移动端响应式适配与 Storybook 全断点 DOM 验收 | 已完成 | `pc6du-mobile-responsive-breakpoints/SPEC.md` | 2026-03-04 | PR #54（run #154 checks 全绿）；35 场景 DOM 验收 + 断点边界修正完成 |
| 35uke | 修复付费周期识别：年付被误判为月付 | 已完成 | `35uke-billing-period-detection-fix/SPEC.md` | 2026-03-03 | PR #52（run #145 checks 全绿） |
| 4ccac | 下架产品归档与三态恢复显示（全产品 + 监控页） | 已完成 | `4ccac-delisted-product-archive/SPEC.md` | 2026-03-03 | 归档 API + cleanupAt + 产品页三态筛选 + 监控页默认隐藏归档 + relist 自动恢复 |
| 4tnv8 | 配置卡片点击打开下单页 | 已完成 | `4tnv8-card-click-open-order/SPEC.md` | 2026-03-03 | strict pid 下单页 + sourcePid 恢复 + 分组标题 Iconify link 图标 + 库存为 0 弹窗拦截（spec-sync） |
| pgnnw | 发布链路修复与 GHCR 回填闭环（Dockrev 无候选） | 已完成 | `pgnnw-release-ghcr-chain-fix/SPEC.md` | 2026-02-26 | fast-track + 验证闭环完成 |
| 7ey9f | 懒猫云购物车库存监控 | 已完成 | `7ey9f-lazycats-cart-inventory-monitor/SPEC.md` | 2026-01-20 | UI 对齐 wireframes + 监控页重新同步（Playwright 复验）  |
| hrjpv | Storybook 展示与主题切换（含亮色主题） | 已完成 | `hrjpv-storybook-theme-switching/SPEC.md` | 2026-01-20 | 补齐 stories 覆盖（components/pages/layout）  |
| tbpgt | CI/CD：发版自动化（GHCR + GitHub Release + Release Assets）对标与补齐 | 已完成 | `tbpgt-release-automation-alignment/SPEC.md` | 2026-01-21 | Release assets（4 targets + sha256）+ GHCR multi-arch + PR smoke test + UI embed  |
| grjep | 配置卡片：库存历史与近 1 日走势（minute bucket） | 已完成 | `grjep-inventory-history-trend/SPEC.md` | 2026-01-21 | 实现：history API（batch）+ 卡片背景 sparkline + 30 天清理；API=sparse；>10=10+  |
| 8btwa | CI/CD：自动发版意图标签与版本号策略（防止 docs-only 发版） | 已完成 | `8btwa-release-intent-label-gating/SPEC.md` | 2026-01-21 | PR label gate：`type:docs\|skip\|patch\|minor\|major`；无关联 PR 的 `push main`=跳过；base=语义版本最大 tag（无 tag fallback `Cargo.toml`）；按标签 bump  |
| xrv7y | CI/CD：GitHub Actions 构建提速（PR：跳过 arm64 + gating + cache） | 已完成 | `xrv7y-github-actions-performance/SPEC.md` | 2026-01-22 | PR ≤ 10 分钟（baseline Actions run `21235535755`）；PR smoke job 输出 timings summary（key steps）  |
| yghay | CI/CD：修复 Release job 失败（Determine release version/tag） | 已完成 | `yghay-release-job-unblock/SPEC.md` | 2026-01-24 | M3 证据：run `21313516531`（main，tag `v0.1.9`）+ `21313516840`（tag `v0.1.8`）；`Determine release version/tag` step 成功  |
| uqe6j | 系统设置：通知测试按钮（Telegram + Web Push） | 已完成 | `uqe6j-settings-notifications-test-button/SPEC.md` | 2026-03-04 | Telegram：错误信息增强（description + migrate_to_chat_id/retry_after）并补充 URL-encoded token 脱敏；Web Push：补齐发送链路（VAPID private/subject）用于测试  |
| 2vjvb | 全量刷新：SSE 进度 + 缓存复用 + 配置上下架 | 已完成 | `2vjvb-catalog-full-refresh-sse/SPEC.md` | 2026-01-25 | 全局调度=最小间隔；缺失一次即下架；监控页展示最近 24h 上架（含重新上架）  |
| 6b675 | 配置卡片：国家国旗水印背景 | 已完成 | `6b675-card-country-flag-watermark/SPEC.md` | 2026-01-26 | 实现：国旗水印（Iconify `flagpack`）  |
| ynjyv | 采集观测台：全局采集队列 + SSE 日志订阅 | 已完成 | `ynjyv-ops-collection-dashboard/SPEC.md` | 2026-01-27 | -  |
| wzc6m | 关于：版本号显示 + 升级提示 + 仓库地址显示 | 已完成 | `wzc6m-about-version-update-meta/SPEC.md` | 2026-02-17 | -  |
| jqp64 | 上游站点域名迁移（lxc.lazycat.wiki） | 已完成 | `jqp64-upstream-domain-lxc-lazycat-wiki/SPEC.md` | 2026-02-21 | PR #44  |
| xvecz | Codex：安装 UI UX Pro Max 技能 | 已完成 | `xvecz-install-ui-ux-pro-max-codex/SPEC.md` | 2026-02-26 | - |
