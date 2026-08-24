[English](MAINTAINER.md)

# 维护者指南

本指南面向修改代码、构建发布、调试 Gateway 以及验证桌面端安装包的开发者。它描述当前 HEAD 已实现的 V3 架构与运行契约。

## 章节

- [仓库结构](maintainer/layout.zh-CN.md) — crate 与目录结构。
- [开发](maintainer/development.zh-CN.md) — 前置条件、开发循环、检查与构建。
- [架构](maintainer/architecture.zh-CN.md) — 四层 crate、适配器身份与请求流转。
- [Dashboard API](maintainer/dashboard-api.zh-CN.md) — V3 契约、CAS token 与变更规则。
- [状态、凭据与生命周期](maintainer/state-and-lifecycle.zh-CN.md) — `CoreState`、锁顺序、凭据与持久化。
- [HTTP 路由](maintainer/http-routes.zh-CN.md) — 推理路由、V3 路径、V2 墓碑与 auth/session 路由。
- [存储与迁移](maintainer/storage-migration.zh-CN.md) — SQLite schema v27、备份与运维手册。
- [扩展 OCG Manager](maintainer/extending.zh-CN.md) — 静态密封的供应商扩展步骤。
- [发布产物](maintainer/release-artifacts.zh-CN.md) — 支持的平台矩阵与包名。
- [CI 工作流](maintainer/ci.zh-CN.md) — quality、release 与 container 工作流。
- [发布流程](maintainer/releasing.zh-CN.md) — 版本 bump、tag、构建与发布检查清单。
- [已知缺口与明确非目标](maintainer/known-debt.zh-CN.md) — 已记录的缺口与 deliberate non-goals。
- [编码约定](maintainer/conventions.zh-CN.md) — Ponytail 原则、crate DAG 与安全边界。

## 阅读路径

- **贡献者** — `layout` → `development` → `architecture` → `state-and-lifecycle` → `http-routes` → `conventions`。
- **发版负责人** — `release-artifacts` → `ci` → `releasing` → `known-debt`。
- **UI / 主题工作** — 先读 `DESIGN.md`，再改 `src/theme.ts` 与对应 Vue 页面。

---

[文档索引](README.zh-CN.md) · [English](MAINTAINER.md)
