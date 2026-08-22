# Documentation / 文档索引

OCG Manager documentation is split by audience. Start from the product README,
then open the guide that matches your role. Keep Chinese/English pairs in sync
when you edit a paired page.

OCG Manager 文档按读者拆分。先从产品 README 入手，再打开对应角色的指南。修改
成对文档时请保持中英同步。

## Catalog / 目录

| Audience / 读者 | English | 简体中文 | Scope / 范围 |
| --- | --- | --- | --- |
| Product overview / 产品概览 | [../README.md](../README.md) | [../README.zh-CN.md](../README.zh-CN.md) | What it is, download matrix, 3-step start, pointers into USER |
| End users / 终端用户 | [USER.md](USER.md) | [USER.zh-CN.md](USER.zh-CN.md) | Install, dashboard, Plans, aliases, model tables, gateway behavior, CLI, Docker, limits, troubleshooting |
| Maintainers / 维护者 | [MAINTAINER.md](MAINTAINER.md) | [MAINTAINER.zh-CN.md](MAINTAINER.zh-CN.md) | Layout, dev loop, architecture, release matrix, CI, validation |
| Anti-abuse / 防滥用 | [OPENCODE_GO_ANTI_ABUSE.md](OPENCODE_GO_ANTI_ABUSE.md) | [OPENCODE_GO_ANTI_ABUSE.zh-CN.md](OPENCODE_GO_ANTI_ABUSE.zh-CN.md) | Allowed use boundary for OpenCode-Go |
| Contributors / 贡献者 | [CONTRIBUTORS.md](CONTRIBUTORS.md) | bilingual / 中英同页 | Community credits |
| Design system / 设计系统 | [../DESIGN.md](../DESIGN.md) | English source of truth | Themes, type scale, Key naming, layout rules |
| AI coding agents / AI 助手 | [../AGENTS.md](../AGENTS.md) | 中文 | Project facts and coding constraints for assistants |

Root stubs redirect old top-level paths into this directory:

根目录保留跳转页，把旧路径指到本目录：

- [../CONTRIBUTORS.md](../CONTRIBUTORS.md) → [CONTRIBUTORS.md](CONTRIBUTORS.md)
- [../OPENCODE_GO_ANTI_ABUSE.md](../OPENCODE_GO_ANTI_ABUSE.md) → [OPENCODE_GO_ANTI_ABUSE.md](OPENCODE_GO_ANTI_ABUSE.md)
- [../OPENCODE_GO_ANTI_ABUSE.zh-CN.md](../OPENCODE_GO_ANTI_ABUSE.zh-CN.md) → [OPENCODE_GO_ANTI_ABUSE.zh-CN.md](OPENCODE_GO_ANTI_ABUSE.zh-CN.md)

## Fact ownership / 事实归属

When docs disagree, prefer the source below and fix the other side.

文档冲突时以下列为准，并回修另一侧：

| Topic / 主题 | Source of truth / 权威来源 |
| --- | --- |
| User-visible product behavior / 用户可见行为 | Code + [USER.md](USER.md) / [USER.zh-CN.md](USER.zh-CN.md) |
| Plan catalog / Plan 目录 | `crates/ocg-core/src/provider.rs` (`BUILTIN_PLANS`); USER Accounts mirrors live vs pending families |
| Client aliases / 客户端别名 | `crates/ocg-core/src/alias.rs`; USER Aliases / 别名 mirrors the contract |
| Local `GET /v1/models` / 本地客户端模型列表 | `crates/ocg-core/src/gateway/handler.rs`; authenticated Go aliases ∪ saved Zen Free aliases ∪ eligible Custom IDs; the GET itself makes no upstream request |
| Applications picker list / 应用选择器列表 | `crates/ocg-core/src/dashboard.rs` (`GET /dashboard/api/application-models`); Go routeable aliases ∩ active pricing; no Custom |
| Custom API HTTP / Custom API HTTP | `crates/ocg-core/src/custom.rs` + `custom_http.rs`; trusted-admin destinations, Direct/Manual/Auto, no redirects, isolated auth |
| SCNet official snapshot / SCNet 官方快照 | `crates/ocg-core/src/provider.rs` (`SCNET_TOKEN_PLAN_*`); adapter input only, never client aliases |
| Model preferred/supported protocols / 模型协议表 | `crates/ocg-core/src/gateway/protocol.rs` (`MODEL_PROTOCOLS`); USER Protocol Conversion mirrors the table |
| Model context/input/reasoning capabilities / 模型能力表 | `src/views/application-guides.ts` (`APPLICATION_MODEL_METADATA`); USER Model capabilities mirrors the table |
| Dashboard HTTP API / 面板 API | `crates/ocg-core/src/dashboard.rs` |
| Release artifacts, CI, signing / 发版与签名 | [MAINTAINER.md](MAINTAINER.md) / [MAINTAINER.zh-CN.md](MAINTAINER.zh-CN.md) + `docs/MAINTAINER` CI sections |
| Current package version pins / 版本钉 | `package.json` / workspace `Cargo.toml` / `src-tauri/tauri.conf.json` / `compose.example.yaml` |
| UI copy for the access credential / 接入凭证文案 | Panel shows **Key** (`DESIGN.md`, `src/theme.ts`); never “Gateway Key” |
| Design tokens / 设计 token | [../DESIGN.md](../DESIGN.md) + `src/theme.ts` |
| Agent coding constraints / 助手约束 | [../AGENTS.md](../AGENTS.md) |

Example version in Docker snippets should match the current release line
(currently **v1.8.2**). Do not leave older patch pins in USER /
`.env.example` / `compose.example.yaml` after a version bump. The product
README no longer pins a clone tag.

Docker 示例里的版本钉应与当前发版线一致（现为 **v1.8.2**）。升版后不要把
USER / `.env.example` / `compose.example.yaml` 留在旧 patch。产品 README
不再钉 clone tag。

## Reading order / 阅读顺序

1. **New user** — README quick start → User guide install / first client /
   accounts (Key import vs managed Beta) / true vs false circuit breakers.
2. **Docker / CLI operator** — User guide Docker and CLI chapters; enable the
   browser profile when managed onboarding needs noVNC.
3. **Contributor** — Maintainer guide layout, development, checks; keep
   `AGENTS.md` for project facts (managed wizard, quota refresh, protocol
   table, Key naming).
4. **Release owner** — Maintainer guide release procedure, CI, and validation
   checklist (include managed rewind and refresh-quota paths).
5. **UI / theme work** — `DESIGN.md` first, then `src/theme.ts` and the Vue
   surface you are changing.

1. **新用户** — README 快速开始 → 用户指南的安装、首个客户端、账号（导入 Key /
   托管 Beta）、真/假熔断。
2. **Docker / CLI 运维** — 用户指南的 Docker 与 CLI 章节；托管注册需要时启用
   browser profile。
3. **贡献者** — 维护者指南的仓库结构、开发与检查；编码时以 `AGENTS.md` 为准
   （托管向导、刷新额度、协议表、Key 命名）。
4. **发版负责人** — 维护者指南的发版步骤、CI 与发版前检查清单（含托管回退与
   刷新额度路径）。
5. **UI / 主题** — 先读 `DESIGN.md`，再改 `src/theme.ts` 与对应 Vue 页面。

## Editing rules / 编辑约定

- Keep EN/ZH heading structure and TOC anchors aligned for paired guides.
- Prefer short absolute facts over marketing language.
- Do not invent remote sync, Admin API, embeddings, or unsupported Gemini
  fields; known gaps live in USER Limits and MAINTAINER Known Debt / AGENTS.
  Do not claim live Command Code GOAT or SCNet Token Plan routing, usage,
  pricing, verification, or provider guides. Custom API is live under the
  trusted-administrator boundary in USER; do not revive Phase-1 / SSRF-denylist
  wording. Do not invent a
  `requested_alias` log field. Do not equate `GET /v1/models` with
  `application-models` (the latter is Go ∩ pricing only). Do not claim browser, live-provider-key, or
  installed-desktop proof unless those checks were actually run.
- After release version bumps, update Docker clone tags and image pins in
  USER, `.env.example`, and `compose.example.yaml` together
  (`pnpm run release:check` covers compose/package version alignment).
- Keep the product README a landing page: identity, download, three-step
  start, one curl, a Docker pointer, the preferred-protocol grouping, and
  links into USER. Do not copy the passthrough matrix, capability table, or
  circuit-breaker essay back into README.

- 成对指南保持中英标题结构与 TOC 锚点一致。
- 优先写短而可核验的事实，少写宣传句。
- 不要编造远端同步、Admin API、embeddings 或未支持的 Gemini 字段；已知缺口见
  用户指南「限制」、维护者指南「已知缺口」与 `AGENTS.md`。不要把 Command Code
  GOAT 或 SCNet Token Plans 写成已上线路由、用量、计价、验证或供应商教程。
  Custom API 已在受信管理员边界下上线路由，以 USER 为准，不要再写回 Phase-1
  休眠或 SSRF denylist。不要发明 `requested_alias` 日志字段。不要把
  `GET /v1/models` 与 `application-models` 写成同一份列表（后者只是 Go ∩ 价格）。
  除非实际做过检查，不要宣称浏览器、真实供应商 Key 或已安装桌面版实测。
- 发版升版后，同步更新 USER、`.env.example`、`compose.example.yaml` 中的
  clone tag 与镜像钉（`pnpm run release:check` 会核对 compose/package
  版本一致性）。
- 产品 README 只做入口：定位、下载、三步上手、一条 curl、Docker 指针、推荐
  协议分组，以及指向用户指南的链接。不要把透传矩阵、能力表或熔断长文再写回
  README。
