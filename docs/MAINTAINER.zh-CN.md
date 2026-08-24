[English](MAINTAINER.md)

# 维护者指南

本指南面向修改代码、构建发布、调试 Gateway 以及验证桌面端安装包的开发者。它
描述当前 HEAD 已实现的 V3 架构与运行契约：四层 crate、Dashboard V3、schema
v27、宿主生命周期、测试与发版。用户可见产品行为见 [USER.zh-CN.md](USER.zh-CN.md)。
schema v27 操作恢复见
[MAINTAINER-v3-migration.zh-CN.md](MAINTAINER-v3-migration.zh-CN.md)。

## 目录

- [仓库结构](#仓库结构)
- [环境前置条件](#环境前置条件)
- [开发模式](#开发模式)
- [检查与构建](#检查与构建)
- [架构说明](#架构说明)
- [生命周期类别](#生命周期类别)
- [HTTP 路由](#http-路由)
- [存储与迁移](#存储与迁移)
- [扩展步骤](#扩展步骤)
- [故障模式](#故障模式)
- [升级与数据库迁移](#升级与数据库迁移)
- [发布产物](#发布产物)
- [CI 工作流](#ci-工作流)
- [发版步骤](#发版步骤)
- [发版前检查清单](#发版前检查清单)
- [已知缺口](#已知缺口)
- [明确非目标](#明确非目标)
- [编码约定](#编码约定)

## 仓库结构

```
ocg-manager/
├── crates/
│   ├── ocg-domain/     纯身份、目录、协议策略、Zen 规范化
│   ├── ocg-gateway/    无 I/O 的 Alias、AttemptSpec、classify、selector、JSON 转换
│   ├── ocg-infra/      与目录剥离的 crypto、代理 HTTP、推理 HTTP、SQLite 日志 SQL
│   ├── ocg-core/       组合 / 控制面：state、SQLite、Dashboard V3、适配器、executor
│   ├── ocg-cli/        无头 CLI（`ocg-manager-cli`）：serve / key / status
│   └── ocg-browser-worker/  Linux Chromium Sidecar 控制服务（不依赖 ocg-core）
├── browser/           Xvfb、Openbox、x11vnc、noVNC 启动脚本
├── src/               Vue 3 管理面板（TypeScript、naive-ui、Vite、Pinia）
│   ├── App.vue        外壳、登录、侧边栏、顶栏
│   ├── api/
│   │   ├── dashboard-v3.ts            手写 `/dashboard/api/v3` 客户端
│   │   ├── generated/dashboard-v3.ts  由冻结 JSON Schema 生成的类型
│   │   ├── dashboard.ts               面向现有页面的 V3 presenter
│   │   ├── dashboard-presenters.ts    字段投影（camelCase 线协议 → 页面形状）
│   │   ├── http.ts                    与端点无关的 fetch 辅助
│   │   └── tauri.ts                   历史命名；部分测试仍引用的遗留类型/辅助 —— 不是 Tauri invoke
│   ├── stores/        session、controlPlane（CAS 令牌）、connection、accounts、providers、settings
│   ├── components/    账号卡、托管向导、价格目录、…
│   ├── i18n/          i18n 注册表 + 各语言文案 + 单元测试
│   ├── styles/        主题 token、设计系统覆盖
│   └── views/         Dashboard、Keys、Accounts、Providers、Applications、Logs、Settings、BrowserSession
├── src-tauri/         托盘宿主：原生浏览器、Gateway 生命周期、桌面设置、升级器
│   └── src/host/      进程级能力；没有 `invoke` command
├── schema/            冻结的 Dashboard V3 JSON Schema（`dashboard-api-v3.schema.json`）
├── docs/              USER / MAINTAINER / 防滥用（中英）、CONTRIBUTORS、索引、v27 恢复说明
├── scripts/           release、updater manifest、dashboard-v3-contract、冒烟脚本、…
├── AGENTS.md          给 AI 编码助手的项目事实与约束
├── DESIGN.md          设计系统源（CI 中 lint）
├── .github/workflows/ quality.yml、release.yml、container.yml
├── docker-bake.hcl    container.yml 用来并行构建冒烟镜像的 bake 目标
├── Dockerfile         多阶段无头 Gateway 镜像
├── Dockerfile.browser Chromium/noVNC Sidecar 镜像
├── compose.yaml       支持源码构建与镜像拉取的 Compose 服务定义
└── compose.example.yaml  每个 Release 附带的只拉取镜像示例
```

Workspace 成员在根目录 `Cargo.toml` 声明：`ocg-domain`、`ocg-gateway`、
`ocg-infra`、`ocg-core`、`ocg-cli`、`ocg-browser-worker`、`src-tauri`（包名
`ocg-manager`）。二进制名：`ocg-manager-cli` 与 Tauri 应用。当前 workspace
版本为 `1.8.2`；`rust-version` 为 `1.85.0`；edition 为 `2024`。

Vue 的主数据路径是 HTTP Dashboard V3（`src/api/dashboard-v3.ts` 以及
`src/api/dashboard.ts` 的 presenter）。不存在 `src-tauri/src/commands/`
模块，也没有 `tauri::generate_handler` / `#[tauri::command]` 表面。
`src/api/tauri.ts` 是遗留文件名，部分单测仍用来导入历史类型；它不是
`invoke()`，也不是生产客户端。

## 环境前置条件

使用 Node.js 22（CI 基线）、pnpm 10.29.2（`package.json` 的
`packageManager`）和 Rust 1.85 或更高版本。原生构建依赖随 runner 调整，以
`.github/workflows/release.yml` 为准。当前 Linux runner 安装
`libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev
libxdo-dev libssl-dev patchelf libfuse2 xvfb xauth xdg-utils dbus-x11`。

## 开发模式

先退出 release 托盘程序，释放单实例锁和 `9042` 端口，然后启动完整开发栈：

```bash
pnpm install
pnpm run dev
```

`pnpm run dev` 实际执行 `tauri dev`。Windows 上 `predev` 脚本
（`scripts/free-dev-port.mjs`）会检查 `127.0.0.1:30001` 并清理上一次残留的
Vite 进程。Tauri 启动 Vite，等 Gateway 就绪后打开
`http://127.0.0.1:30001/dashboard/`。Vite 把 `/dashboard/api`（含
WebSocket）代理到 `http://127.0.0.1:9042`。

- 前端（Vue、CSS、TypeScript）改动走 Vite HMR。
- Rust 改动走 Tauri watcher + Cargo 增量编译，然后重启进程。Rust 代码 **不会**
  在进程内热替换，需要重启。

克隆后启用一次共享 git hooks（`pnpm install` 的 `prepare` 脚本也会执行）：

```bash
pnpm run hooks:install
# 等价：git config core.hooksPath .githooks
```

当本次提交暂存了任意 `*.rs` 文件时，`.githooks/pre-commit` 会运行
`cargo fmt --all`，并把这些 Rust 文件重新 `git add`，保证提交内容符合
rustfmt（与 CI 的 `cargo fmt --all -- --check` 同一套工具）。

## 检查与构建

```bash
pnpm install
pnpm run test
pnpm run build:web
pnpm run design:lint
pnpm run contract:v3:check
pnpm run build
```

- `pnpm run build:web` 是 **纯前端** 生产构建（`vue-tsc && vite build`），只
  验证面板时用它。
- `pnpm run test` 跑 `pnpm run test:web`（Node `--experimental-strip-types`
  覆盖 `scripts/*.test.mjs` 与 `src/**/*.test.ts`）、`vue-tsc --noEmit`、
  `vite build`，然后 `cargo test --workspace --locked`。
- `pnpm run test:rust` 单独跑锁定依赖的 workspace Rust 套件。
- `pnpm run contract:v3:check` 用 `ocg-core` 的
  `export_dashboard_v3_schema` example 重新生成 Dashboard V3 JSON Schema，
  若 `schema/dashboard-api-v3.schema.json` 或
  `src/api/generated/dashboard-v3.ts` 漂移则失败。写入用
  `pnpm run contract:v3:generate`。
- `pnpm run design:lint` 用 `@google/design.md` lint `DESIGN.md`。
- `pnpm run build` **只用于发版验证**。它会跑 `scripts/release.mjs`，为当前
  支持的原生平台构建 GUI 与 CLI，并在每个产物都通过校验后原子替换
  `release/`。失败时旧 `release/` 保留。Cargo 增量编译缓存不会被清空。发版
  二进制使用 thin LTO（workspace `Cargo.toml` 的 `[profile.release]`），把
  原生 CI 链接时间控制在可接受范围。

### Rust 检查

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace --locked
```

第一条命令只检查格式，不修改文件；需要格式化时运行 `cargo fmt --all`。启用
hooks 后，暂存了 Rust 文件的 commit 会由 `.githooks/pre-commit` 自动执行
格式化。

聚焦工作：

```bash
cargo test -p ocg-domain
cargo test -p ocg-gateway
cargo test -p ocg-infra
cargo test -p ocg-core
cargo test -p ocg-manager-cli
cargo test -p ocg-browser-worker
cargo test -p ocg-manager --lib
cargo test -p ocg-core gemini
cargo test -p ocg-core claude_desktop
cargo test -p ocg-core dashboard_v3
cargo test -p ocg-core v3_runtime_invariants
```

`ocg-domain` / `ocg-gateway` 把生产源码的依赖与纯度守卫编成普通
`cargo test`。宿主刻画矩阵见
`crates/ocg-core/tests/fixtures/v3/requirement_map.md`，以及
`src-tauri/tests/fixtures/v3/host_requirement_map.md` /
`crates/ocg-cli/tests/fixtures/v3/host_requirement_map.md` 的副本。

测试真实账号流时，先在沙箱里跑 CLI：

```bash
ocg-manager-cli --data-dir /tmp/ocg-cli-test key add smoke sk-smoke
ocg-manager-cli --data-dir /tmp/ocg-cli-test key list
ocg-manager-cli --data-dir /tmp/ocg-cli-test serve --port 19042
```

CLI 表面只有 `serve` / `key` / `status`。`key add` 通过
`account_control::create_go_api_key` 创建启用且 ready 的 OpenCode Go 卡，并
bump 该进程的 `settings_revision`。它不能创建 Custom 账号、子 Key 或设置。
直接 `Database::update_account` 仍不 bump revision；这是有意的，也不是 CLI
路径。

### 前端检查

前端单元测试与代码放在同一目录（`src/**/*.test.ts`），用 Node 实验性的
`--experimental-strip-types` 跑，不需要额外测试框架。脚本级测试在
`scripts/*.test.mjs`（发版辅助、Dashboard V3 契约、容器发布）。最后再跑
`pnpm run build:web` 与 `pnpm run contract:v3:check`。

应用教程由 `src/views/application-guides.ts` 的 16 个条目驱动；改动注册表时
同时检查教程数量、唯一 ID、协议端点、display/copy 脱敏差异，以及 Claude
Desktop 三个角色模型的持久化行为。

侧栏是仪表盘、接入 Key、账号、供应商、应用、日志、设置。`pricing` 查询是
供应商页的遗留别名。`BrowserSession` 是会话层，不是第八个侧栏项。

## 架构说明

### 四层 crate

供应商扩展是 **静态且封闭** 的。没有插件槽、JSON DSL 或用户自定义适配器。
Custom API 是 `ProviderAdapterKind::ConfigurableHttp`，不是其他适配器继承的
基类。

```text
ocg-gateway -> ocg-domain
ocg-core    -> ocg-domain + ocg-gateway + ocg-infra
ocg-cli     -> ocg-core
src-tauri   -> ocg-core
```

`ocg-domain` 与 `ocg-infra` 都不依赖内部 `ocg-*` crate。
`ocg-browser-worker` 是独立进程，不依赖任何内部 crate。

| Crate | 负责 | 不得持有 |
| --- | --- | --- |
| `ocg-domain` | ID、`BUILTIN_PLANS`、`ProviderAdapterKind`、协议表、Zen ID 规范化、账号/步骤枚举 | DB、`CoreState`、reqwest、rusqlite、tokio、axum、文件系统、时钟 |
| `ocg-gateway` | Alias 注册表、`AttemptSpec`、classify 策略、无密钥 selector、整文档 JSON 转换 | DB、`CoreState`、原始 reqwest、rusqlite、axum、明文凭据 |
| `ocg-infra` | Key 混淆、与目录剥离的代理客户端、推理 HTTP 辅助、单语句日志 SQL | 产品目录、`AppConfig`、Dashboard DTO |
| `ocg-core` | SQLite、`CoreState`、Dashboard V3、供应商适配器、`GatewayExecutor`、`forward_once`、用量同步、宿主组合 | 插件注册表；适配器仍不得持有 DB/`CoreState`/原始客户端 |

`ocg-core` 用 **显式兼容门面** 保留历史公开路径（`alias.rs`、`provider.rs`、
`crypto.rs`、`http_client.rs`、`kernel/{ids,catalog,protocol,zen}.rs`、
`gateway/{attempt,classify,protocol,selector}.rs`）。不要 glob 再导出
`ocg_domain` / `ocg_gateway` / `ocg_infra`。`kernel/mod.rs` 的生产图守卫要求
DAG，**没有多节点 SCC**。`redaction.rs` 是 crate 级叶子。`db` 不依赖
`pricing` 或 `gateway_keys`。`dashboard_v3` 不导入 `gateway` 或 `dashboard`。
`account_control`、`gateway_keys` 与 `usage_sync` 不点名 `CoreState`。

`ocg-gateway` 生产依赖恰好是 `anyhow`、`base64`、`ocg-domain`、
`serde_json`。`ocg-domain` 生产依赖恰好是 `chrono`（仅 serde+std，无 clock
feature）、`serde`、`serde_json`、`sha2`。

### ocg-core 作为组合 / 控制面

`ocg-core` 把其他 crate 接起来。只有它打开 SQLite、持有 `CoreStateInner`、
挂载 HTTP、访问上游。

- `host_router.rs` 是 HTTP 组合根：推理路由 + `/dashboard/api/v3` + 已退役
  V2 REST 墓碑 + 面板静态资源。`gateway` 不导入面板挂载。
- `host_gateway.rs` 实现 `GatewayRebindHost`，让 `state` 在不导入 `gateway`
  的情况下重绑监听器。
- `gateway_runtime.rs` / `routing_runtime.rs` 是 DAG 叶子，在 `gateway` 与
  `state` 之外持有 `GatewayHandle` 与路由槽。
- `account_control.rs` 是与 HTTP 无关的账号变更服务。Dashboard V3 用 CAS
  包装它；CLI 调用同一组函数，argv 上没有 CAS 令牌。两者在成功持久化后都
  bump `settings_revision`。
- `gateway_keys.rs` 拥有 `access_keys` 表与内存凭据快照。具体 `KeyStore` /
  `KeyHost` 实现在 `state`。
- `control/observability.rs` 是与 HTTP 无关的本地读取逻辑，供遗留 V2 适配器
  与 V3 共用。它不发出站 HTTP。

### Gateway 执行

客户端推理在 `crates/ocg-core/src/gateway/`。Axum + Tokio + reqwest，默认
绑定 `127.0.0.1:9042`。鉴权前请求体上限 16 MiB。

职责拆分：

1. **`handler.rs`** — 请求 id（`x-ocg-request-id`）、凭证鉴权、客户端解析/
   格式校验、Claude Desktop 改写、Alias 解析。然后把已解析、已解析 Alias
   的请求交给 executor。
2. **`GatewayExecutor`** — 冻结的请求入口快照、候选选择、同账号重试、账号
   回退。一个逻辑客户端请求从头到尾使用同一份不可变价格 revision、同一份
   `ForwardRouteSet`、同一份合约集、同一次 Alias 解析。每次回退迭代仍 **重新
   读取** 账号、合格 Custom 运行时与 Zen Free 冷却。
3. **`provider_adapter.rs`** — 对封闭的 `ProviderAdapterKind` 穷尽匹配。返回
   纯数据的 `AttemptSpec`（URL、路径、上游协议、鉴权方案、重定向策略、不透明
   `CredentialHandle`、`ProxyRoutingModel`）。适配器接收账号、配置与请求
   plan。它们 **不** 解密 Key、不打开数据库、不构建 HTTP 客户端。
4. **`forwarder.rs` / `forward_once`** — 每次调用恰好一次上游 `.send()`。
   只负责传输选择与超时。`forward_once` 内没有策略、没有重试、没有回退。
5. **宿主 `CredentialResolver`** — 在外层循环已经选中账号之后再解密 handle。

鉴权收集 Bearer / `x-api-key` / `x-goog-api-key` 全部非空候选头。任一命中
`CoreStateInner.credential_snapshot`（主 Key 与启用子 Key）即通过；按候选头
顺序首次命中归因。该快照也是转发日志名称来源。客户端凭据在出站前剥离，只
注入所选账号已配置的方案。不要把 Gemini 或 Anthropic 客户端凭据透传到上游
offering。不得把 Command Code / GOAT 别名到 OpenCode，也不得把 GOAT Key 发到
OpenCode endpoint。

标准入口：`/v1/chat/completions`、`/v1/responses`、`/v1/messages`、
`/v1/models`。Claude Desktop：`/claude-desktop/v1/messages` 与
`/claude-desktop/v1/models`。Gemini 同时接受 `/v1beta/models/{model}:*` 与
`/v1/models/{model}:*`；`generateContent` / `streamGenerateContent` 进入转换
链；`countTokens` / `embedContent` 返回 `501`；未知 action 返回 `404`。带鉴权
的 `GET /v1/models` 是当前可路由已公布别名（OpenCode Go 与最后一次成功 Zen
Free 快照）加上合格 Custom 声明 ID 的 **本地** 读取——**零上游发现**。受保护的
`GET /dashboard/api/v3/application-models` 是另一份本地列表：Go 可路由别名 ∩
当前 Go 价格快照（highspeed 变体继承基价行；空交集为 `[]`）。它不含 Custom
ID。Claude Desktop `/claude-desktop/v1/models` 仍然只公布三个角色别名。

Alias 注册表在 `ocg-gateway::alias`（门面 `ocg_core::alias`）。首选别名是稳定
的小写 kebab-case（沿用现有 OpenCode Go ID）。大小写折叠的 kebab 拼写可接受；
含 `/`、`_` 或空白的名称视为原始 ID，不得折叠成 kebab 别名。原始 ID 在注册表
中恰好对应一个 mapping 时钉在该 mapping；之后才检查可路由性，因此不可路由
mapping 会被识别但不能产出生产路由。重叠的原始 ID 返回 `400`，错误码
`ambiguous_model_id`，且不得调用上游。未知名称在 Chat Completions、
Responses、Messages 以及 Gemini generate / streamGenerate 上返回 `400`。合格
Custom ID 会 overlay 进解析与 `/v1/models`，但不得抢走已公布 Go/Zen 别名。已
公布 kebab 别名 `deepseek-v4-flash` 仍归 Go；原始 ID
`deepseek/deepseek-v4-flash` 钉在不可路由的 GOAT。转发日志持久化
`requested_model`、`resolved_alias`、`upstream_model`、`provider_id` 与
`offering_id`。没有 `requested_alias` 字段。

JSON 转换在 `ocg-gateway::protocol`；宿主 `gateway/protocol.rs` 保留解析、
usage、流式与路由身份类型。Gemini 只是客户端格式，绝不是上游协议。已知模型
使用 `ocg-domain` 中硬编码的 `MODEL_PROTOCOLS`（`preferred` + `supported`）：
客户端协议在 `supported` 内则透传，否则转到 `preferred`。未知模型在所有受支持
的客户端格式上直接 `400`，请求路径禁止试探协议。非空 `safetySettings` 必须
`400`；空数组可以接受。`topK`、`thinkingConfig` 只是兼容提示，不得宣称与
Gemini 等价。

`materialize.rs` 只解析一次客户端协议，解析 Alias，再按候选物化 model /
protocol / endpoint / auth。适配器不得用可计费推理路径试探协议支持。OpenCode
`MODEL_PROTOCOLS` 表仍只服务 Go。表中未知的动态 Zen `-free` ID 默认按 Chat
物化。Custom 按账号把协议、隔离 origin 与鉴权方案重新物化为该卡声明值。

`zen_models.rs` 是唯一 Zen Free 模型发现路径。受保护的供应商页显式刷新通过
全局代理请求固定无 Key endpoint `https://opencode.ai/zen/v1/models`，不跟随
重定向，只保留合法且以 `-free` 结尾的 ID；完整成功快照先持久化，再切换运行
时。每个模型同时公布原 ID 与去掉 `-free` 的 Alias。失败或过滤结果为空时保留
旧快照，`/v1/models` 只读取这份快照。Go 所有的 `ox-alpha-free` 是保留排除项。

选择器：宿主 `gateway/selector.rs` 按能力、enabled/ready、凭据有效性、冷却
与本次已失败账号过滤卡片，然后无密钥的 `ocg-gateway::selector` 状态机按该
顺序行走（`StrictPriority` / `StickyGlobal` / `RoundRobin`）。不要引入模型
路由页或按模型额度池。Zen free 额度按出口 IP 共享：任一有效
`cooldown_free_until` 即视为整条 free 通道耗尽（不换 Key）。

价格快照不可变且按供应商分范围。刷新只在用户点击时发生。对 OpenCode Go，
月额度只用来推导账号额度扣减倍率（`月额度 / Usage`），它不是可路由额度池。
官方表中 Input/Output/Usage 全是 `-` 的行（目前 Ox Alpha Free /
`ox-alpha-free`）按无价格的 Go 促销跳过。官方倍率与当前值不同时，先返回不
激活的差异预览；后续请求同时绑定当前 revision 与刚预览的官方 content hash。
抓取器仅允许 OpenCode Go HTTPS 主机和同主机重定向，总时限 20 秒、响应体上限
2 MiB。MiniMax 长上下文、priority 和 high-speed 调整是本地策略。

回退 / 重试（executor + classify，**不是** `forward_once`）：

- 只有能证明请求尚未发出的 DNS/TCP/TLS 建连失败可以在同一账号重试 **一次**，
  且必须发生在任何下游字节之前。
- 部分 SSE 不得回退。无法确认的流式结果记为 `outcome_unknown`。
  `StreamOutcomeGuard` 在 drop 时收口。
- OpenCode（Go/Zen）推理 `401` 原样返回：不换号、不写 `auth_error`（Go 会把
  模型不存在也打成 401）。普通 Custom `401` 换号并持久化 `auth_error`。面板
  Ping / Key 验证的 401 仍记录 `auth_error`。
- `403` 与 Go 通道 `429` 可以切换账号。free 通道 `429` 冷却按 IP 共享的 free
  池，不换 Key，并按持久化卡片顺序继续尝试后续兼容候选。普通 Custom/GOAT
  `429` 不解析 Go 窗口。
- `408`、`5xx`、建连后的失败、响应体超时和流式中断均不得重放。
- 共享 reqwest client 只设置 30 秒建连超时；非流式请求使用 900 秒总时限，
  流式请求按 chunk 执行 300 秒空闲时限。

`AttemptSpec` 上的 `ProxyRoutingModel`：

- `RequestEntrySnapshot` — 冻结的双段 `ForwardRouteSet`（Go / Zen）。跟随
  重定向。受限 URL（https 或回环 http）。
- `ProcessWideNoRedirect` — 仅 GOAT 回环测试。生产 GOAT 在没有回环守卫时
  fail-closed。
- `IsolatedTrustedAdmin` — Custom：进程级代理、禁重定向、不转发客户端头、
  管理员受信 URL。

全局出站代理是进程级（`AppConfig`）：自动 / 手动 HTTP / 强制直连 / List。
List 模式使用 `proxy_list_direction` 与 `proxy_list_models`。名单内模型走
方向例外段（白名单→代理 / 黑名单→直连）；名单外模型与非模型出站（验证、
Zen 刷新、用量、价格、升级下载）走方向默认段。名单成员校验只在 dashboard
`update_settings` 写闸口执行（非空、精确已知 id、去重）；加载路径容忍旧值。
构造在 `ocg-infra::http`；`ocg-core::http_client` 在精确匹配前折叠目录别名。
请求从入口持有一份 `ForwardRouteSet`；并发设置切换只影响之后启动的请求。

### Plan 目录

`BUILTIN_PLANS` 与 `ProviderAdapterKind` 在 `ocg-domain::provider`（门面
`ocg_core::provider`）。五个家族：

| 家族 | ID | 可路由 | 说明 |
| --- | --- | --- | --- |
| OpenCode Go | `opencode` / `go` | 是 | 只接受官方分发 Key |
| Zen Free | `opencode-zen-free` / `anonymous-free` | 是 | 无凭据单例，数据库持有 |
| Command Code GOAT | `command-code` / `goat` | 否 | 禁用 `pending` 草稿；验证 `501` |
| SCNet Token Plans | `scnet` / `token-plan-basic\|standard\|premium` | 否 | `sk-tp-` 前缀；验证 `501` |
| Custom API | `custom` / `api` | 是 | 受信管理员目的地 |

所有持久化变更路径都会在改动行、revision 或时间戳之前，拒绝为目录内
`routable=false` offering 设置 `enabled=true`。每次 `Database::open` 都会禁用
遗留的已启用 GOAT 与全部三个 SCNet tier，且不改 `updated_at`。Custom 的
enabled 状态予以保留。未验证 GOAT 重置为 `pending`。Go、Zen Free 和未知
pair 不受影响。

Custom API（`custom.rs` + `custom_http.rs`）：接受任意语法合法的 HTTP 或
HTTPS 源（含局域网、回环与自选目的地）；拒绝 URL 内嵌凭据、query 与
fragment。永不跟随重定向；永不转发 dashboard 或客户端鉴权；只构造已配置的
Bearer 或 `x-api-key`。拼接 endpoint 必须保持 scheme、host、port 与
base-path 前缀。超时把 `connect_timeout_secs` 夹到 5–60 秒。创建/更新后仍为
禁用 `pending`。验证对第一个声明模型发一次协议正确的最小非流式请求；只有
`2xx` JSON object 成功；不发现或改写能力，永不自动启用。验证成功后需显式
启用。Key、base URL 或能力变更会使验证失效并禁用账号；协议与鉴权方案创建后
不可改。Custom 费用/用量为 unpriced/unknown，不扣供应商额度。

SCNet 官方可用模型快照 `2026-08-21`（大小写与顺序必须与代码一致，只作适配器
输入，不得作为 `model_aliases`）：`GLM-5.2`、`GLM-5`、`GLM-5.1`、`Kimi-K3`、
`Kimi-K2.7-Code`、`Kimi-K2.6`、`Kimi-K2.5`、`DeepSeek-V4-Flash`、
`DeepSeek-V3.2`、`MiniMax-M3`、`MiniMax-M2.7`、`MiniMax-M2.5`、
`MiMo-V2.5-Pro`。价格表 / FAQ 里 **不在** 该表的额外名称：`DeepSeek-V4-Pro`、
`DeepSeek-V4-Flash-0731`、`Qwen3.8-max`、`Qwen3-235B-A22B`。文档记载的 base 是
`https://api.scnet.cn/api/llm/v1` 与
`https://api.scnet.cn/api/llm/anthropic`。风险确认 id
`scnet-token-plan-restrictions`，版本 `2026-08-21`。本 crate 不得对 Token
Plan 发真实请求。

### Dashboard V3

当前面板 JSON 是 `/dashboard/api/v3`，与已退役 V2 REST 墓碑并列挂载。线协议
DTO 使用 camelCase，变更体 `deny_unknown_fields`，可空响应字段始终序列化为
`T | null`。

控制面身份：

- `settings_revision` — `CoreState` 上的内存 `AtomicU64`，成功持久化后 bump。
  CAS 令牌本身不存 SQLite。
- `process_generation` — 每个 `CoreState` 赋值一次，永不持久化。上一进程的
  CAS 令牌在重启后不能复用。
- `pricingRevision` — 不可变快照 id。价格变更还要带
  `expectedPricingRevision`。

变更要求顶层 `expectedRevision` **和** `processGeneration`（包括
`/auth/register`、`/auth/login`、`/auth/logout` 以及
`POST /accounts/{id}/usage/refresh`）。缺少 `expectedRevision` 是专门的
`400` `missingExpectedRevision`。不匹配是 `409` `revisionConflict`，错误信封
带 `currentRevision` / `processGeneration`。Vue `controlPlane` store 从每个
V3 载荷记录令牌。预期的 409 恢复是从 `GET /contract` 刷新令牌且不重放变更，
但当前客户端仍检查旧的蛇形错误码 `revision_conflict`；见“已知缺口”。revision 与
generation token 只属于当前进程，不能协调共用同一数据目录的多个进程。

不是变更（无 CAS、不 bump）：`POST /settings/test-proxy`、
`POST /custom/models/discover` 这类操作探测。`GET /settings/check-update` 与
`GET /settings/update-status` 捕获 revision/generation 且永不 bump。
`POST /settings/install-update` 需要 CAS，原子启动，不 bump，不持有网络/DB
锁。

密钥边界：明文 Key 不得出现在 `Settings` 或供应商/Zen/合约 DTO 上。
`ConnectionInfo`（`GET /connection`）是 **唯一** 携带密钥的 V3 响应 DTO（主
Key 与所有未软删的子 Key 值，包括禁用子 Key，处于 dashboard 会话保护层）。只有
启用的 Key 会进入鉴权快照。
`CustomModelDiscoveryRequest.apiKey` 只写。账号 list/get 载荷保持无密钥。
日志与错误信封脱敏已知密钥。

冻结契约是 `schema/dashboard-api-v3.schema.json`，由
`dashboard_v3::contract_schema_pretty()` 经
`crates/ocg-core/examples/export_dashboard_v3_schema.rs` 生成。生成的
TypeScript（`src/api/generated/dashboard-v3.ts`）只有类型，没有 HTTP 封装。
`dashboard_v3/types.rs` 的 `CATALOG_TYPE_NAMES` 是有序 `$defs` 目录；追加时
既有 definition 对象必须保持字节一致。

前端：Pinia store 直接调用 `dashboardV3`。仍使用旧字段名的现有页面走
`src/api/dashboard.ts` presenter。不要加 V2 导入、路由回退或递归大小写转换。

`dashboard.rs` 仍包含历史 V2 REST 处理器，并提供面板 HTML/资源。这些受保护
REST 处理器 **不是** 现行 API：`host_router` 会先拦截已退役的
`/dashboard/api/...` 路径。

### 已退役的 V2 REST

受保护的 Dashboard V2 REST 已退役。

- 匿名已退役 REST：空 body 的 **401**（鉴权先于墓碑）。
- 已鉴权的已退役 REST（含回环本地模式）：**410**，body 为
  `{ "code": "dashboardV2Removed", "message": "Dashboard API V2 has been removed; refresh the page and retry." }`。
- 既非 V3 也非保留家族的未知 `/dashboard/api/...` 路径，在已鉴权时同样 410。

保留的 `/dashboard/api` 家族（精确路径，无尾斜杠，无额外段）：

- `auth/status`、`auth/register`、`auth/login`、`auth/logout`
- `browser/sessions/{token}/ws`（token 非空）

V3 在 `/dashboard/api/v3/...` 下有自己的鉴权与浏览器 WebSocket。当前 Vue
外壳使用 V3。推理路由、面板 HTML 与 `/dashboard/assets/...` 不在墓碑范围内。

### 状态、凭据与设置

`CoreStateInner`（`state.rs`）由 Gateway、面板与 CLI 共享。

锁顺序：(1) `settings_update`，(2) `db`，(3) `config`，(4) `http_client`，
(5) `gateway`，(6) `pricing`，(7) `zen_free_models`，(8)
`provider_contracts`，(9) `routing`，(10) `credential_snapshot`。禁止反向获
取。不要在持有 routing 锁时做 DB 或网络 I/O。异步闸口：设置写同时重绑时，
`settings_host_effects`（持久化 → 监听器重绑 → 补偿）先于
`gateway_lifecycle`。不要在这些 await 期间持有 `parking_lot` 锁。

两层凭证共用一张 `access_keys` 表（schema v27）和一份鉴权快照：

- 主 Key：固定 id `00000000-0000-0000-0000-000000000001`，显示名
  `"Primary"`。始终启用，永不删除。公开 `AppConfig` 与面板 API 仍暴露
  `gateway_key`；v27 之后经消毒的 config JSON **不再** 是该值的数据库权威。
- 子 Key：非主行，活跃上限 64，软删保留身份/名称并清除明文。只经
  `/dashboard/api/v3/keys*` 生命周期 API 变更。CLI 没有子 Key 命令。

主/子 Key 值互斥由 `gateway_keys::ensure_primary_value_allowed` 在
dashboard、settings 与子 Key 启用路径强制。

`AppConfig` 使用 serde 默认值做向后兼容加载。1.3 之前没有
`claude_desktop_models` 的配置会得到默认 Sonnet 目标 `minimax-m3`，并被规范
写回。常规 settings 保存会保留专用的 Claude Desktop 映射。下游访问根地址
优先级：非空 `OCG_CLIENT_ROOT_URL`（只读，不得写回）> SQLite 手工值 > 前端
按生产 origin / 开发 Gateway 端口自动推导。

**回环监听时** 直接访问跳过登录。带标准反向代理转发头但没 Cookie 的请求仍
需登录。**非回环监听** 走单管理员模型：密码以 Argon2 哈希存 SQLite，登录下
发 HttpOnly 会话 Cookie。Docker 可用 **同时设置的** `OCG_ADMIN_USERNAME` 与
`OCG_ADMIN_PASSWORD` 引导首个管理员；只设一个会启动报错；不提供时由首位
注册者创建。

设置页通过 `GET /dashboard/api/v3/settings/check-update` 获取 GitHub
Release 元数据。支持升级的已安装桌面运行时可继续下载、校验签名并安装；开发
构建、CLI、Docker 只保留元数据/发布页路径。出站请求只在用户点击按钮时发起。

### 账号生命周期与浏览器运行时

schema v16 给账号增加 `account_type`（`key | managed`）与 `setup_step`
（`google_account → opencode_registration → payment → key_verification → ready`）。
旧行迁移为 `key + ready`。托管草稿立即持久化为空 Key、`enabled=false`；选择
器、启用接口和路由都必须同时要求 `ready` 与非空 Key。步骤名 `google_account`
在 UI 上展示为「登录身份」，可跳过。

`AppConfig::default()` 的 `opencode_invite_url` 带演示默认值
（`DEFAULT_OPENCODE_INVITE_URL`）。规范化后只接受最长 2048 字符、无用户名密码
的 HTTPS URL，主机严格限定为 `opencode.ai` 或 `console.opencode.ai`。创建托管
草稿时可编辑邀请链接；与设置不同时写回 SQLite。注册/支付/验证码仍由用户在
浏览器中完成，Key 由用户复制回填；**不得** CDP 自动填表或代点支付。

托管状态允许 **向前一步** 或 **回退到任意更早的未完成步骤**；禁止跳步前进，
禁止经 setup API 直接进入 `ready`。Key 实测返回 `2xx` 时进入
`ready + enabled`；`429` 同样证明 Key 有效并写入冷却；`401`/`403`、网络错误
或 `5xx` 保持 `key_verification`。

官方 Go usage（`go_usage.rs`，`https://opencode.ai/zen/go/v1/usage`）是校准基
线，由 `usage_sync.rs` 协调。手动
`POST /dashboard/api/v3/accounts/{id}/usage/refresh` 与后台对账共用同一条
fetch + key CAS + 三窗口校准路径。ready+enabled 且近 24h 有本地活动的账号约
每小时对账，无活动约每天；禁用 / 非 ready / 空 Key 排除。启动不得轰鸣：全局
并发 1、节奏控制、有界抖动，并提供可注入 clock/jitter/fetch 缝。手动刷新在
任何尝试后有 15 秒每账号节流、并发去重与 Retry-After / `nextAllowedAt`。本地
最大 Go 用量 ≥80% 时最多每 15 分钟加速一次。真实推理 `429` 仍写现有
cooldown/selector，并额外调度约 1–2 分钟后的官方同步（绝不 inline）。官方
失败或 `status=rate-limited` 永不写推理冷却。成功后按最早 `resetsAt`（有界
抖动）调度，同时尊重活跃/非活跃节奏。失败退避：5m → 15m → 1h → 6h；永不清除
last-success 或上次基线。sync 元数据在 `provider_usage_sync_state`（v27 删除
遗留的五列 `accounts.usage_sync_*`）。公开 Go docs 尚未列出该路径。

`console_usage.rs` **已冻结** 弃用——勿调用、勿扩展；至少两个 minor 且有稳定
真号证据后再删。手工滑块 / PATCH 校准仍然可用。

Zen Free 由数据库持有：可启用、停用、排序，但不能通过通用账号 API 创建或
删除。GOAT / SCNet 草稿保持禁用且不可路由。Custom 在验证后显式启用即可路由。

浏览器：`GET /dashboard/api/v3/browser/capabilities`、
`POST /accounts/{id}/browser`、`DELETE /accounts/{id}/browser-profile` 与
`/browser/sessions/{token}/ws`。浏览目标允许 Google 注册/登录、GitHub 注册/
登录、配置的邀请 URL 与 OpenCode 控制台（`https://opencode.ai/auth`）。
worker 主机白名单含 `accounts.google.com`、`github.com`、`opencode.ai`、
`console.opencode.ai`、`auth.opencode.ai`。远程会话令牌只在内存中保存，绑定
管理员会话并检查 Origin，空闲 30 分钟或总计 4 小时失效。

桌面原生浏览器 hook 由 `src-tauri/src/host/` 注册进 `CoreState`。Vue 仍通过
HTTP 调用。Windows 依次查 Edge、Chrome；macOS 查 Chrome、Edge、Chromium；
Linux 从 `PATH` 查 Chrome/Chromium/Edge。外部浏览器使用
`browser-profiles/<account_id>`、`--no-first-run`、
`--no-default-browser-check` 与新窗口，不得加入 CDP、automation、
`--no-sandbox` 或关闭 Web 安全的参数。

`crates/ocg-browser-worker` 每节点只保留一个 Chromium。切换账号先 SIGTERM
当前进程组并等待 Profile 写盘，超时才强制结束。Sidecar 以 UID/GID 10001、
只读根文件系统、零 capability 运行；控制 token 由共享运行时卷随机生成。
Chromium 需要建立自身的 user/PID/network namespace 和 renderer seccomp
沙箱，因此 browser 服务使用 `seccomp=unconfined` 且不能启用
`no-new-privileges`。Sidecar 仍不挂载 SQLite，不发布宿主机端口。浏览器项目
桥接网络不能设为 Docker `internal`，因为 Chromium 需要访问 Google/OpenCode
的 HTTPS 出站网络。

Profile 删除必须先停浏览器，校验账号 ID 防目录穿越，再把新旧 Profile 原子
改名暂存；数据库操作成功后清理暂存目录，失败则恢复。重置完成账号不删除
Key；重置注册中账号还要回到 `google_account`。删除账号的 UI 确认必须写明
Cookie/Profile 也会删除。

### 持久化

`crates/ocg-core/src/db.rs` 定义 SQLite schema、迁移与查询。当前 schema 是
**v27**。`provider_contracts.rs` 负责供应商合约范围与模型协议证据。
`models.rs` 定义共享 serde 类型和 `AppConfig`。Key 混淆在
`ocg-infra::crypto`（门面 `ocg_core::crypto`）：这是轻量混淆，不是 KMS。
Windows 桌面使用 `MachineBoundCipher`；CLI/Docker 使用来自
`OCG_MANAGER_ENCRYPTION_KEY` 或 `<data-dir>/.encryption-key` 的
`StaticKeyCipher`。生产宿主必须调用 `Database::open_with_cipher`，让 v27
密文探测使用已经解析的 cipher。账号 `key_cipher` / `password_cipher` 就地
校验，**永不重新加密**。比本构建支持的更新 schema 会 fail closed。

升级路径上历史版本仍然重要：

- v16：托管 setup 列。
- v21：usage-sync 元数据（v27 从 `accounts` 迁走）。
- v22：不可变 provider/offering 绑定、供应商价格/用量、额度窗口、供应商感知
  转发日志。首次把早于 v22 的库迁到 v22 时写
  `data.sqlite.pre-v22.<UTC>.bak`。
- v23：Plan 验证状态、别名 / 上游日志身份、可选原生成本、Custom 配置表、
  SCNet 确认。首次把早于 v23 的库迁到 v23 时，在任何 v23 写入前写
  `data.sqlite.pre-v23.<UTC>.bak`。
- v24：转发日志新增实际代理路由段（`auto` / `proxy` / `direct`；历史空串=
  未记录）。
- v25：`provider_model_catalogs`（Zen Free 最后一次成功快照）。
- v26：`provider_contract_scopes` 与 `provider_contract_model_protocols`。
  加法迁移，不单独创建 pre-v24/v25/v26 备份。
- **v27：** 把主 `gateway_key` 与 `sub_gateway_keys` 复制进 `access_keys`；
  删除 `sub_gateway_keys`；删除遗留 `accounts.usage_sync_*`。数据库到达规范
  v26 后，既有（非空）库会在 **任何 v27 写入前** 得到唯一同目录副本
  `data.sqlite.pre-v3.<UTC>.bak` 及其 SHA-256 sidecar。全新空目录直接创建
  v27，不写这份副本。操作恢复见
  [MAINTAINER-v3-migration.zh-CN.md](MAINTAINER-v3-migration.zh-CN.md)。

GUI 数据目录：Windows `%USERPROFILE%\.ocg-mgr` 或 macOS/Linux `~/.ocg-mgr`。
CLI 默认 `~/.ocg-mgr-cli`。Docker 将 SQLite、Key 与 `.encryption-key` 放在
`ocg-data`，长期 Cookie 与浏览器状态放在 `ocg-browser-profiles`。两卷都是高
敏感持久状态，必须在服务停止后成对备份；`ocg-browser-runtime` 只含运行时
控制 token，不应加入备份。浏览器 Profile 不由 OCG Manager 加密。

转发日志插入走 `ocg-infra::sqlite_logs`（每个辅助恰好一条显式语句）。调用方
拥有时间戳、诊断、费用策略、脱敏与事务。

### 节点边界

每个节点由自己的面板独立管理；不提供跨节点同步，也不提供 Admin API。不要
新增。

## 生命周期类别

这四类必须分开。不要用一类去取消另一类。

| 类别 | 启动 | 停止 | 说明 |
| --- | --- | --- | --- |
| **Gateway 监听器**（`GatewayLifecycle`） | `start_gateway` / `bind` | `stop`（只发信号）或 `stop_and_wait`（CLI） | TCP 绑定、面板信任、转发日志回填、HTTP 服务。重绑感知槽位（同端口先停后绑，新端口先绑）。不启动也不取消进程级 worker。 |
| **控制面 worker**（`ControlPlaneWorkers`） | 由 `start_gateway` 调用 `ensure_started`（每个 `CoreState` 一次） | 无 —— 拥有该 `CoreState` 被 drop 时退出 | 官方用量对账。没有公开 cancel API。监听器停止不得杀死它。 |
| **桌面能力** | Tauri setup：自启（仅 Windows release/已安装）、Dock（macOS）、升级 starter | 进程退出 | 不是 WebView command。CLI/Docker 不注册 hook。HTTP 设置表单仍按能力门控 `auto_start` 与 `show_dock_icon`。 |
| **浏览器运行时** | 桌面原生 hook；Docker 远程 worker | 账号切换 / Profile 重置 / 进程退出 | 原生浏览器与 Sidecar 是同一 `BrowserRuntime` 槽的不同宿主。 |

Tauri `src/lib.rs`：启动用 `start_gateway`（监听器 + 用量 worker）；退出用
`host::gateway::stop_listener`（只停监听器）。设置端口变更经
`GatewayLifecycle` / `settings_host_effects` 重绑，并用配置指纹做补偿；并发
失败的端口写入不得覆盖成功的超时写入。

升级器注册为 `CoreState` starter，绝不是 WebView `invoke` command。
`src-tauri/capabilities/default.json` 没有 updater 权限。升级器出站遵循进程
级 **默认段** 代理策略（含 List 模式）。

## HTTP 路由

### 推理（路径未改）

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| POST | `/v1/chat/completions` | OpenAI Chat |
| POST | `/v1/responses` | OpenAI Responses（无状态；`store` / `previous_response_id` / `conversation` / `background` → 400） |
| POST | `/v1/messages` | Anthropic Messages |
| GET | `/v1/models` | 本地列表；需要鉴权 |
| POST | `/claude-desktop/v1/messages` | 角色别名改写后走 Messages |
| GET | `/claude-desktop/v1/models` | 三个角色别名 |
| POST | `/v1beta/models/{model}:*` 与 `/v1/models/{model}:*` | Gemini 客户端格式 |

### Dashboard V3（`/dashboard/api/v3`）

公开：`/auth/status`、`/auth/register`、`/auth/login`、`/auth/logout`。

会话保护（非穷尽；见 `dashboard_v3/mod.rs`）：`/contract`、`/connection`、
`/settings`、`/settings/test-proxy`、`/claude-desktop/models`、
`/settings/check-update`、`/settings/update-status`、
`/settings/install-update`、`/pricing`、`/pricing/refresh`、
`/pricing/multipliers`、`/providers/{provider_id}/{offering_id}/pricing`、
`/keys`、`/keys/primary/regenerate`、`/keys/{id}`、`/keys/{id}/regenerate`、
`/accounts`、`/accounts/managed`、`/accounts/order`、`/accounts/{id}`、
`/accounts/{id}/toggle`、`/accounts/{id}/browser`、
`/accounts/{id}/browser-profile`、`/accounts/{id}/setup`、
`/accounts/{id}/setup/verify-key`、`/accounts/{id}/reset-cooldown`、
`/accounts/{id}/custom-config`、`/accounts/{id}/model-capabilities`、
`/accounts/{id}/acknowledgements`、`/accounts/{id}/usage`、
`/accounts/{id}/usage/refresh`、`/accounts/{id}/provider-usage`、
`/accounts/{id}/verify`、`/providers`、`/providers/model-capabilities`、
`/providers/zen-free`、`/providers/zen-free/models`、
`/providers/zen-free/models/refresh`、`/provider-contracts`、
`/provider-contracts/provider/{scope_id}/protocols/{protocol}`、
`/providers/{provider_id}/protocol-probes`、`/browser/capabilities`、
`/browser/sessions/{token}/ws`、`/gateway/status`、`/application-models`、
`/dashboard/summary`、`/dashboard/daily-cost-by-model`、`/logs/gateway`、
`/logs/forward`、`/logs/forward/models`、`/logs/forward/keys`、
`/custom/models/discover`。

Go/Zen 协议探测是 `POST /providers/{provider_id}/protocol-probes`。Custom
在该路径被拒绝（`protocol probes for Custom API are account-owned`）。历史
V2 `POST /accounts/{id}/protocol-probes` 为 410。Custom 连接验证是
`POST /accounts/{id}/verify`；模型发现是操作探测
`POST /custom/models/discover`。

### 静态面板

`GET /dashboard`、`GET /dashboard/`、`GET /dashboard/assets/{*path}`。

## 存储与迁移

见 [持久化](#持久化) 与
[MAINTAINER-v3-migration.zh-CN.md](MAINTAINER-v3-migration.zh-CN.md)。操作摘要：

1. 停止所有打开该数据目录的进程。WAL 文件与 `data.sqlite` 同属一份库。
2. 保留匹配的加密密钥（Windows 机器绑定材料，或
   `OCG_MANAGER_ENCRYPTION_KEY` / `.encryption-key`）。不同 cipher 会 fail
   closed，不得靠改写密文“修好”。
3. 用更新二进制打开前备份整个数据目录，包括存在时的 `.encryption-key` 与
   `browser-profiles/`。
4. v27 只为既有 v26 库写 `data.sqlite.pre-v3.<UTC>.bak` + `.sha256`。恢复前
   校验 sidecar。
5. 只把 pre-v3 文件恢复到具备 v26 能力的二进制，或从该 v26 快照重试 v27。
   不要让 v26 二进制打开 schema 27。
6. 失败的 v27 事务会回滚；源库必须仍为 v26。已有的 pre-v3 文件留在原地——
   之后成功的 open 会再建一个唯一文件名，而不是覆盖第一份。

项目不保证降级兼容；如需回滚，恢复对应旧版本升级前的数据备份，不要让旧
二进制直接打开已迁移的数据库。

## 扩展步骤

### 新增或修改供应商（封闭）

1. 在 `ocg-domain`（`ids.rs`、`provider.rs`）加入身份与目录事实。穷尽扩展
   `ProviderAdapterKind`（`ALL`、`from_offering`、能力组合）。Custom 保持
   `ConfigurableHttp`，不要做成超类。
2. 若该家族需要协议行，加在 `ocg-domain::protocol`。不要在请求路径试探协议。
3. 若该家族需要别名，加在 `ocg-gateway::alias`。不可路由 mapping 可以被识别
   但不产出生产路由。
4. 在 `ocg-core` 为新 kind 实现 `resolve_route`，只返回 `AttemptSpec`。
   **适配器不能持有 DB、`CoreState` 或原始 reqwest 客户端。** 解密与 HTTP 留在
   宿主 resolver / `forward_once`。
5. 在路由、验证、用量、计价真正实现之前 fail closed。GOAT/SCNet 是「目录在、
   未上线」的模板。
6. 跑 `cargo test -p ocg-domain`、`cargo test -p ocg-gateway` 与
   `cargo test -p ocg-core`。纯度/依赖守卫会拦住禁止的导入。

不要加插件加载器、动态库或用户提供的适配器脚本。

### 新增或修改 Dashboard V3 端点

1. 在 `dashboard_v3/types.rs` 增加或扩展 DTO，并把新名字追加到
   `CATALOG_TYPE_NAMES`。不要改既有 `$defs` 对象。
2. 在 `dashboard_v3/mod.rs` 挂路由。变更走 `parse_mutation_json` +
   `check_expectation`。保持密钥边界。
3. 优先复用 `account_control` / `gateway_keys` / `control::observability`，
   不要复制持久化逻辑。不要从 `dashboard_v3` 导入 `gateway`。
4. 在 `crates/ocg-core/tests/dashboard_v3_*.rs` 补集成测试。
5. 跑 `pnpm run contract:v3:generate`（CI 用 `--check`），并更新
   `src/api/dashboard-v3.ts` 手写客户端。只有现有页面仍需要旧形状时才改
   `dashboard.ts` / `dashboard-presenters.ts`。
6. 不要复活 `/dashboard/api` REST。新的受保护 JSON 属于 V3。

### 新增宿主能力

桌面能力放在 `src-tauri/src/host/`，注册进 `CoreState`。不要重新引入
`#[tauri::command]`。Vue 必须继续走 HTTP。

## 故障模式

| 现象 | 可能原因 | 处理 |
| --- | --- | --- |
| 面板 JSON `410` `dashboardV2Removed` | 客户端仍在调用 `/dashboard/api/...` REST | 刷新/升级页面；改用 `/dashboard/api/v3` |
| 面板 JSON `409` `revisionConflict` | 过期的 `expectedRevision` / `processGeneration` / `expectedPricingRevision` | 重新加载资源；不要自动重放变更 |
| 面板 JSON `400` `missingExpectedRevision` | 变更体漏了 CAS | 发送 `expectedRevision` + `processGeneration` |
| `/dashboard/api/...` 上空 body `401` | 匿名已退役 REST 或缺少会话 | 登录；回环只对 **直接** 请求跳过登录 |
| Gateway `400` `ambiguous_model_id` | 原始 ID 映射到多个家族（含 Custom 重叠） | 改名/避开冲突的 Custom ID；不得调用上游 |
| Gateway `400` 未知模型 | 名称既非已公布别名也非合格 Custom ID | 使用 `/v1/models`；不要试探协议 |
| 推理 `401` 原样返回、不换号 | OpenCode Go/Zen 的 `ModelError` 或无效 Key | 预期行为；Ping/验证仍会记 `auth_error` |
| Zen `429` 冷却所有 Free 卡 | 出口 IP 共享池 | 等待 `cooldown_free_until`；后续非 Free 卡仍可能运行 |
| `success_no_usage` | 上游未发出 usage chunk | Chat 流式会请求 `include_usage`；没有 chunk 时该行用量缺失 |
| 打开失败：schema 新于 27 | 数据目录来自更新的二进制 | 恢复匹配备份；不要用旧二进制打开 v27 |
| 打开失败：cipher / 密文 | 错误的 `.encryption-key` 或机器绑定上下文 | 恢复匹配密钥；永不改写密文 |
| 中断的 v27 open | 事务已回滚；pre-v3 备份可能已存在 | 见 [MAINTAINER-v3-migration.zh-CN.md](MAINTAINER-v3-migration.zh-CN.md) |
| 设置改端口后仍绑旧端口 | 重绑失败；补偿恢复了配置 | 查 gateway 日志；并发写入由 `settings_host_effects` 串行 |
| `stop_gateway` 后用量循环仍在跑 | 监听器停止不会取消 `ControlPlaneWorkers` | drop `CoreState`（进程退出） |

## 升级与数据库迁移

GUI 或 CLI 启动时会原地执行 SQLite 迁移。升级前备份完整数据目录，包括数据
库、存在时的 `.encryption-key` 与 `browser-profiles/`；Docker 同时备份
`ocg-data` 和 `ocg-browser-profiles`。直接/手动升级时先停止进程，签名桌面
升级器会自行停止并重启。项目不保证降级兼容；如需回滚，恢复对应旧版本升级前
的数据备份，不要让旧二进制直接打开已迁移的数据库。

schema v23 还会在任何 v23 写入前生成已校验的同目录副本
`data.sqlite.pre-v23.<timestamp>.bak`。schema v27 在规范 v26 之后、任何 v27
写入之前写入 `data.sqlite.pre-v3.<UTC>.bak` 及其 SHA-256 sidecar（仅既有
库）。请与常规备份一起保存，直到确认新安装可用。它们只是回滚点，不是完整
备份，也不是允许旧二进制打开已迁移数据库的许可。

v1.4.1 既没有升级运行时，也没有内置签名校验公钥。Windows 的一次性过渡需要
明确指导用户：退出托盘程序，运行首个支持升级的 setup，在“升级方式”页选择
第二项 **不要卸载，直接安装（Install without uninstalling）**。第一项只是
Tauri 默认选中项，并非升级所必需；不要先卸载 v1.4.1。高级用户可选择执行等
价命令：

```powershell
Start-Process -FilePath .\ocg-manager_<version>_windows-x64-setup.exe -ArgumentList '/UPDATE','/P','/R' -Wait
```

macOS/Linux 按各自常规方式直接替换一次。此后的桌面版可走设置页签名升级。
CLI 与 Docker 仍手动升级。

## 发布产物

支持的发布矩阵刻意保持精简：

| Runner | GUI | CLI |
| --- | --- | --- |
| Windows 10/11 x64 | NSIS 当前用户安装包 | x64 ZIP |
| macOS 11+ | Universal DMG（x64 + ARM64） | Universal tar.gz |
| Linux x64 | AppImage + deb | x64 tar.gz |

稳定的产物命名：

```text
ocg-manager_<version>_windows-x64-setup.exe
ocg-manager_<version>_windows-x64-setup.exe.sig
ocg-manager-cli_<version>_windows-x64.zip
ocg-manager_<version>_macos-universal.dmg
ocg-manager_<version>_macos-universal.app.tar.gz
ocg-manager_<version>_macos-universal.app.tar.gz.sig
ocg-manager-cli_<version>_macos-universal.tar.gz
ocg-manager_<version>_linux-x64.AppImage
ocg-manager_<version>_linux-x64.AppImage.sig
ocg-manager_<version>_linux-x64.deb
ocg-manager_<version>_linux-x64.deb.sig
ocg-manager-cli_<version>_linux-x64.tar.gz
compose.example.yaml
latest.json
SHA256SUMS
```

每个 CLI 压缩包都包含可执行文件、`dist/`、`LICENSE`。**不要** 只发 CLI 可执
行文件：`serve` 需要同级的 `dist/`。Windows 没有 portable GUI 安装包。

`linux/amd64` 与 `linux/arm64` 容器单独发布为 `ghcr.io/klarkxy/opencode-go-mgr`。
GitHub Release 包含七份常规平台 payload、额外的 macOS 升级压缩包、四份升级
签名、只拉取镜像的 Compose 示例、`latest.json` 与 `SHA256SUMS`（当前共 15
个附件）。本地验证器固定校验当前这 15 个文件，工作流同时要求 GitHub 附件的
名称与数量和组装后的 `release/` 集合完全一致。运行镜像内的许可证位于
`/usr/share/licenses/ocg-manager/LICENSE`。

### scripts/release.mjs

`scripts/release.mjs` 负责所有繁重工作：

1. 校验 `package.json`、`src-tauri/tauri.conf.json`、workspace `Cargo.toml`、
   `src-tauri/Cargo.toml`，以及 `compose.example.yaml` 的三个带版本字段
   （标题、主镜像和浏览器镜像默认值）一致；如有 Git tag，与之比对。
2. 在创建暂存目录前解析升级签名模式；设置 `OCG_REQUIRE_UPDATER_ARTIFACTS=1`
   时，缺私钥或 `TAURI_UPDATER_PUBLIC_KEY` 都会在替换 `release/` 前失败；配置
   的公钥还必须匹配 `src-tauri/updater-public-key.sha256` 中已提交的 SHA-256
   连续性基线。
3. 配置签名密钥时，合并 `src-tauri/tauri.updater.conf.json` 和临时公钥配置，
   启用 Tauri 升级产物。`TAURI_SIGNING_PRIVATE_KEY` 可直接填写私钥内容或仓库外
   的安全路径，不另设 path 变量。没有签名密钥时保持普通本地构建，并明确提示该
   结果只适合冒烟，不是可发布的升级版本。
4. 拒绝不支持的 host/arch 组合（`process.platform`/`process.arch`）。
5. 用绝对 bundle 路径调用 `@tauri-apps/cli`：Windows 走 `nsis`，Linux 走
   `appimage,deb`。macOS 普通本地构建走
   `--target universal-apple-darwin --bundles dmg`；启用升级签名时走
   `--bundles app,dmg`，因为 Tauri 只有在构建 `app` target 时才会生成升级压缩
   包。
6. 每份 payload/签名在暂存前都使用实际 `TAURI_UPDATER_PUBLIC_KEY` 做密码学验
   证，再收集 NSIS、AppImage 签名与 macOS `.app.tar.gz`/签名；deb 不是 Tauri
   原生升级产物，因此显式执行 `tauri signer sign`。公私钥即使都非空但不匹配，
   也会 fail closed。
7. 构建 CLI 二进制，与 `dist/`、`LICENSE` 一起打成对应平台的压缩包；macOS 上
   用 `lipo` + `codesign -` 拼出 universal CLI。
8. 对暂存 `release/` 目录内的每份 payload 与签名写 `SHA256SUMS`。
9. 原子替换 `release/`。任意步骤失败，旧 `release/` 保留，暂存目录清理。

`scripts/release.mjs` **不会** 清空 Cargo 增量编译缓存——多次发布共用同一个
`target/`。

`pnpm run release:check` 校验版本、Compose 与已配置签名密钥，不构建原生安装
包。无密钥预检先覆盖未签名契约；生产 tag push 中，每台 runner 都会先用
repository signing secret 签一个临时 payload，并用已通过连续性检查的
`TAURI_UPDATER_PUBLIC_KEY` 验证，再开始昂贵的原生构建。

## CI 工作流

### quality.yml —— 可复用质量门

`.github/workflows/quality.yml` 在 PR 和 `main` push 上自动运行，`release.yml`
发版时只调用一次。质量门拆成三个并行 job，前端失败不必等 Rust，Windows 也不
再重做 dashboard 构建：

- **Web** —— `pnpm run contract:v3:check`、Node 测试（`scripts/*.test.mjs`
  与 `src/**/*.test.ts`）、TypeScript 检查、Vite 生产构建、`DESIGN.md` lint
  与 Compose 校验。
- **Rust** —— `cargo fmt`、锁定依赖的 workspace 测试与 Clippy，用占位
  `dist/index.html` 满足 tauri-build，以便 Linux 上仍编译桌面 crate。只有这个
  job 安装 WebKit 头文件。
- **Windows Tauri** —— 对 `ocg-manager` 跑 `cargo test --lib`/`clippy`，用占位
  `dist/index.html` 满足 tauri-build，覆盖 Windows 专属自动启动，不再装 pnpm
  或跑 Vite。

兼容的运行共享 Node/pnpm 和 Rust 构建缓存；PR 只恢复 Rust 缓存，不写回。非 PR
失败时仍会写回 Rust 缓存，方便后续修复复用编译结果。

### release.yml —— 候选与 tag 发布

`.github/workflows/release.yml` 由 `workflow_dispatch` 和 `v*` tag 触发。

- 手动候选可选 Windows x64、macOS Universal、Linux x64 或全部平台，刻意只生成
  未签名冒烟产物；即使手动运行选择 tag 作为 ref，也不会获得生产签名权限。
- 只有 `v*` tag 的 `push` 事件才会强制走完整三平台矩阵并注入 repository signing
  secrets。对这个单维护者仓库，推送该 tag 就是明确的公开发布授权。
- 质量门与无密钥 Ubuntu 预检并行：预检在 `pwsh` 下解析抽出的安装器冒烟脚本、
  运行发布辅助测试并校验所有版本清单。

预检通过后，每个选中的原生 runner 恢复对应 Rust 缓存并安装依赖。工作流只有在
plan 根据事件确认这是真实 `v*` tag push 时才注入签名 secrets，随后验证公私钥和
已提交公钥指纹，再执行带签名构建；手动 job 得到空签名值，只执行普通未签名构
建。两条路径都会运行 CLI/GUI 冒烟并上传保留 7 天的 `release-<platform>`。通用
测试、类型和 lint 不再在三台 runner 上重复执行。

### 各 runner 的冒烟流程

- **Windows CLI**——校验 `SHA256SUMS`，解压 ZIP，对临时 data dir 跑
  `key add` / `key list` / `key disable` / `key enable` / `status` /
  `key remove`，启动 `serve --port=19042` 后等 dashboard HTML 中出现
  `id="app"`。
- **macOS / Linux CLI**——同样的 `key` 与 `serve` 流程；macOS 上额外用
  `lipo -archs` 校验 universal 二进制。
- **Windows GUI**——下载当前已发布安装包，静默安装并启动，写入数据哨兵并启用
  `auto_start`；不卸载旧版，直接用 `/UPDATE /P /R /ARGS --startup` 运行候选
  NSIS，确认旧 PID 退出、`/settings/update-status` 返回候选版本、哨兵与
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\OCG Manager` 都保留。
  安装器进程有显式超时，并与 `/R` 拉起的常驻 GUI 分开等待，避免成功重启反而卡
  住 CI；卸载完成也有时间上限，并通过已安装文件消失等后置条件判断。随后继续自
  启关闭/恢复检查，静默卸载并确认用户数据仍在。PowerShell 实现在
  `scripts/smoke-windows-release.ps1`，不再内嵌在 YAML。手动触发且候选版本已经
  是 latest 时，可走仅安装候选版的路径。
- **macOS GUI**——挂载 DMG，`codesign --verify --deep --strict`，
  `lipo -archs` 校验 universal，`--startup` 启动后等 dashboard。
- **Linux GUI**——`dpkg-deb --info` / `dpkg-deb --contents` 校验 deb，`file`
  校验 AppImage；用 `dbus-run-session -- xvfb-run -a env
  APPIMAGE_EXTRACT_AND_RUN=1 WEBKIT_DISABLE_COMPOSITING_MODE=1` 启动后等
  dashboard。

`scripts/smoke-windows-release.ps1` 当前探测遗留 V2 URL
`http://127.0.0.1:9042/dashboard/api/settings/update-status` 与
`/dashboard/api/settings`。在本架构下这些已鉴权路径返回 410；V3 候选必须对
`/dashboard/api/v3/settings/update-status`（以及对应的 V3 settings 读取）冒
烟。依赖该脚本做发布冒烟前，必须先把它更新到 V3 路径。

### draft-release 与 verify-release

`v*` tag 触发时，下游 `draft-release` job 下载三个 runner 的 Actions
artifact，把平台 payload、签名与 `compose.example.yaml` 组装进 `release/`，
生成使用不可变 tag URL 和 bundle 感知平台键的 `latest.json`，再重写覆盖
manifest、签名和其余附件的 `SHA256SUMS`，最后创建或更新 **draft** GitHub
Release。`verify-release` 随后要求 GitHub 附件名称与组装后的 `release/` 集合
逐名一致；本地验证器还固定校验当前 15 个文件，再重新推导 `latest.json`、重算
全部 checksum、验证四份升级签名，并把每个下载文件与 GitHub Release 存储层报告
的 digest 对比。draft job 会把数字 Release ID 传给下游；验证和公开 job 都重新
校验该 ID、tag 与 draft 状态，不使用无法显示 draft Release 的 tag 查询端点。

`v1.5.8-beta.1` 这类 SemVer 预发布 tag 走同一条真实签名 tag 路径，并保持与组
装产物逐名一致的不可变附件集合。升级 manifest 会在 payload 文件名和下载 URL
中保留完整预发布后缀，Windows 安装包冒烟也接受同一个预发布
`CandidateVersion`。自动生成的说明开头会显著标注“托管账号注册与隔离浏览器
Profile 均为 Beta，尚未充分测试”，并列出尚未实测的 Google/OpenCode 真实注册
与支付、noVNC 键盘/剪贴板、GHCR 首次公开发布路径；同时说明 preview 还包含
Gateway、脱敏和发布链路改动，不能视为生产可用。之后生成稳定版说明时会跳过同
版本预发布 tag，以前一个稳定版为基线，避免完整功能范围被 Beta tag 隐藏。

### publish-release —— 只公开已验证的 tag 构建

`v*` tag push 是单维护者的明确发版授权，因此 `verify-release` 成功后会自动运行
`publish-release`。发布 job 会再次比对当前资产/digest 集合指纹与已验证指纹；
验证后 draft 有任何变化都会拒绝发布。手动候选无法进入 draft、验证或公开发布
job。缺少签名密钥、冒烟失败或验证失败时，Release 都不会公开。

发布 job 还进入仓库级 `release-moving-channels` 串行队列；正式公开前会比较候
选版本和当前 GitHub latest，只允许严格更高的稳定 SemVer 推进 `latest`。延迟
完成的旧 run 仍可公开自己的不可变 Release，但不能把移动通道回滚。
预发布 tag 的 draft 与最终 Release 都设为 `prerelease=true`，固定
`make_latest=false`，且不会调用只适用于稳定版的 latest 比较；稳定 tag 的行为
不变。

### 升级签名密钥

生产升级密钥只在可信工作站生成一次，并写到仓库外的安全路径（不要把仓库内路
径传给此命令）：

```powershell
node node_modules/@tauri-apps/cli/tauri.js signer generate -w <仓库外安全路径>/ocg-updater.key
```

- 私钥内容与密码分别保存为 repository Actions secrets
  `TAURI_SIGNING_PRIVATE_KEY` 与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。发布工
  作流只会在事件派生的 plan 确认真实 `v*` tag push 时引用它们；手动候选得到空
  值并保持未签名。
- Repository secrets 不具备 Environment 隔离；如果以后增加有写权限的维护者，
  应在下一次发版前重新评估受保护签名 Environment 或 tag ruleset。
- 私钥和密码都至少保留两份独立存放的加密备份。它们一旦丢失，已经信任对应公
  钥的客户端就无法再走应用内升级，只能重新直接安装引导版本。
- 公钥可安全分享；本项目通过 repository Actions variable
  `TAURI_UPDATER_PUBLIC_KEY` 注入其内容，而不提交到仓库。GitHub 中保存的是生
  成后的密钥内容，不是本地文件路径。
- 升级签名证明 payload 由本项目发布，但不等同于操作系统代码签名。

### 密钥连续性与轮换

`src-tauri/updater-public-key.sha256` 是生产信任连续性的已提交锚点，正常 CI
没有绕过开关：repository variable 不匹配时，签名预检和 Release 验证都会 fail
closed。密钥轮换属于 break-glass 恢复，不是普通 secret 更新。必须先生成并备份
新密钥、为所有既有客户端准备直接安装引导，再在明确的安全审查变更中更新已提交
指纹；不能只改 variable 或只改指纹，旧安装版无法信任仅由新密钥签出的版本。

### container.yml —— 镜像流水线

`.github/workflows/container.yml` 接受 Release 发布事件，但由 `release.yml` 使用
`github.token` 公开的 Release 不会递归启动另一个工作流。签名 tag 流水线公开
Release 后，稳定版必须对该 tag 显式触发 `container.yml`，并设置
`publish_latest=true`。该工作流检出 Release tag，在各架构原生 runner 上构建
（amd64 用 `ubuntu-24.04`、arm64 用 `ubuntu-24.04-arm`，发布产物不经 QEMU 模
拟），并通过 `docker-bake.hcl` 并行构建本架构的冒烟镜像：主服务
`ghcr.io/klarkxy/opencode-go-mgr` 与 Sidecar
`ghcr.io/klarkxy/opencode-go-mgr-browser`。主镜像冒烟检查 Dashboard、鉴权和许可
证；浏览器镜像在只读根文件系统、零 capability、Chromium 可用的 seccomp
配置、无宿主机端口下启动 Xvfb/noVNC，并通过受 token 保护的控制 API 真正拉起
普通 Chromium 与持久 Profile。

全部验证通过的产物——每架构两个镜像——先按 digest 推送而不分配可变名称，再进入
仓库级串行标签队列。只有 `resolve` job 可以解析请求 tag 或可选 `source_ref`；两个
原生架构 build leg 都检出它输出的完整 commit SHA，并断言 `HEAD` 必须完全相同。
publish job 则使用不可变的 `github.workflow_sha`，确保带 registry 写权限的 helper
来自已审查的 workflow 定义，而不是热修 ref 中的可执行文件。

写入用户可见标签前，publish job 先用 `docker buildx imagetools create --dry-run`
在本地组装两个候选 OCI index，对返回的原始 JSON 求 digest，并校验两个架构 child
及 index 的 version/revision 注解。主镜像与浏览器镜像的 `X.Y.Z`、`sha-<12 位
commit>` 四个不可变标签必须全部以本地已知 digest 完成预检，之后才按浏览器在先、
主镜像在后的顺序创建并验证；已存在标签只有 digest 与候选完全相同时才接受。

随后工作流必须用空 Docker 凭据目录匿名拉取两个精确版本标签，并为两个最终 index
digest 成功写入 GitHub 签名 provenance。只有这些步骤全绿，同一个串行 job 才会
重新读取并预检远端移动通道。稳定版 `X.Y` 和选择更新的 `latest` 要么让两个镜像
都收敛到候选版本，要么保留已经对齐的较新版本对；推进时浏览器在先、主镜像在后，
若仍会分裂则 fail closed。每个架构镜像还记录 SPDX SBOM 与 BuildKit SLSA
provenance。`X.Y.Z` 和 `sha-*` 是不可变发布标签；`X.Y` 与 `latest` 是单调移动
通道。浏览器镜像是 GHCR 包，不会增加 GitHub Release 附件；原生发布只保留组装
后的 GitHub 附件（校验器从该集合推导名称/数量）。

Package 可见性独立于关联仓库管理，工作流不能依赖 repository token 代为改成
公开；新的浏览器 package 在首次推送 digest 前也根本不存在。因此第一次创建该
package 的 `container.yml` 会先完成推送，再因 GitHub 默认的私有可见性停在匿名
拉取门禁。这是唯一允许的引导例外：在 GitHub Package 设置中把新浏览器 package
设为**公开**（并确认主 package 也是公开），然后对同一个 tag 手动重跑
`container.yml`。不可变标签只有 digest 完全相同时才允许重放，所以重跑只完成
原发布，不会替换产物。在重跑全绿前，不得把容器发布视为完成；之后每个 Release
都必须在第一次运行时直接通过匿名门禁。

首次走这条双架构链路发布稳定版之前，必须先发布一个临时 SemVer prerelease，并以
`publish_latest=false` 触发 `container.yml`。这次 rehearsal 要证明两个原生
runner、package 可见性、匿名拉取、index 精确 children 和两份签名 provenance
全部成立。不得拿稳定 tag 当演练，也不得在预发布全绿之前推进 `X.Y` 或 `latest`。

标签发布后，门禁使用空 Docker 凭证目录匿名拉取两个精确版本标签；package 若仍
为私有或不可访问，`container.yml` 会失败，而不会伪装成可供公开 Compose 使用的
成功发布。

手动触发可回填已有 Release tag，且只有显式选择后才会更新 `latest`。`resolve`
显式检出 `refs/tags/<tag>`（或明确指定的热修 `source_ref`），校验 Release tag 与
仓库版本后输出唯一的完整 SHA；后续 job 不再解析符号 ref。若重建内容与既有完整
版本或 `sha-*` 标签的 digest 不同，会失败而不是覆盖；只接受完全相同 digest 的
重放。它的 GitHub 签名证书记录发起 dispatch 的 workflow ref，即使构建随后检出
的是已解析的 release commit。因此不要把历史手动回填描述成“由该 tag 触发的
provenance”；正常 `release.published` 使用 Release tag 上下文。

发布后记录 digest，并同时核验 OCI index 与 GitHub attestation；验证时约束到
本仓库的 signer workflow：

```bash
docker buildx imagetools inspect ghcr.io/klarkxy/opencode-go-mgr:X.Y.Z
docker buildx imagetools inspect ghcr.io/klarkxy/opencode-go-mgr-browser:X.Y.Z
docker buildx imagetools inspect --raw \
  ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest>
docker buildx imagetools inspect --format '{{json .SBOM}}' \
  ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest> > sbom.json
gh attestation verify \
  oci://ghcr.io/klarkxy/opencode-go-mgr@sha256:<digest> \
  --repo klarkxy/opencode-go-mgr \
  --signer-workflow klarkxy/opencode-go-mgr/.github/workflows/container.yml
gh attestation verify \
  oci://ghcr.io/klarkxy/opencode-go-mgr-browser@sha256:<browser-digest> \
  --repo klarkxy/opencode-go-mgr \
  --signer-workflow klarkxy/opencode-go-mgr/.github/workflows/container.yml
```

SBOM 与 provenance 是供应链元数据，不等于漏洞扫描。GitHub attestation 签名的
是 provenance statement；项目当前没有另加独立 Cosign 镜像签名。

当前 Windows 安装包未签名，macOS 用 ad-hoc 签名（`-`），没有 Developer ID
公证。推送 release tag 前必须复核原生候选冒烟和这些平台警告，因为 tag 工作流
成功后会自动公开。Windows / Linux ARM64、32 位 x86、RPM、Snap、应用商店包仍
不支持。签名的应用内升级只用于支持升级的已安装桌面版；v1.4.1、开发构建、CLI、
Docker 仍走直接/手动路径。

### CI 覆盖边界

PR 会自动运行三路并行质量门：前端检查（含 Dashboard V3 契约）、Linux
workspace Rust 测试/Clippy（含 Tauri crate），以及覆盖 Windows 专属 Tauri 行为
编译和单测的 Windows job；原生安装包/打包冒烟仍只在手动候选或 tag 流程运行。
容器工作流覆盖 `linux/amd64` 与 `linux/arm64`（各自在原生 runner 上构建并冒
烟），并且只在 Release 发布后或手动触发时运行。

CI 不会操作真实桌面 UI，也不启动真实 Claude Desktop 或 Gemini CLI，不测试备份
恢复、数据库降级、迁移回滚、真实上游账号或真实 Gateway 请求。
Rust 测试覆盖 Gemini/Claude Desktop 路由、鉴权、别名改写、非流式转换、SSE 事
件形状、Dashboard V3 CAS、V2 410 墓碑、v27 open/备份以及宿主生命周期源码契
约，但不能证明第三方客户端的新版本仍接受生成的配置。容器冒烟只检查 TCP 健康、
Dashboard HTML、auth status、镜像内许可证，以及未登录 settings 返回 `401`。
浏览器容器冒烟会启动真实 Chromium、确认 Profile 目录和无公开端口，但不登录
Google/OpenCode、不操作 noVNC 键鼠/剪贴板，也不执行真实支付。Google 数据中心
IP 风控、桌面浏览器发现、Cookie 跨重启保留和远程账号切换仍需手工验证。

## 发版步骤

1. 确定 `X.Y.Z`（或 `X.Y.Z-beta.N` 这类不可变 SemVer 预发布版本），同步修改
   `package.json`、`src-tauri/tauri.conf.json`、
   workspace `Cargo.toml`、`src-tauri/Cargo.toml`，以及
   `compose.example.yaml` 的标题、主镜像与浏览器镜像默认值。
2. 运行 `cargo check --workspace --all-targets` 刷新 `Cargo.lock`，再运行
   `pnpm install --frozen-lockfile`、`cargo fmt --all -- --check`、
   `pnpm run test`、`pnpm run design:lint`、`pnpm run contract:v3:check`、
   `pnpm run release:check` 和 `pnpm run build`。提交预期的 lockfile 改动，
   不要手工编辑 lockfile。
3. 与上一个公开 tag 比较，复核 diff 和当前平台的 `release/` payload，然后提交
   版本、lockfile、文档与 Release notes 改动。
4. 先合并已经审查的改动，再在 `main` 的最终 commit 上执行
   `git tag -a vX.Y.Z -m "OCG Manager vX.Y.Z"`（如为预发布，保留对应后缀）创建
   附注 tag 并推送。不要在之后还会 squash merge 的分支 commit 上提前打 tag。
5. 等待 `quality`、`preflight`、全部原生矩阵 job、`draft-release`、
   `verify-release` 和 `publish-release` 通过。确认公开的是同一个已验证 draft，
   再复核与组装产物逐名一致的附件集合、冒烟日志、平台警告，以及基于上一个 tag
   diff 编写的说明。
6. 对已发布 tag 显式触发 `container.yml`（例如
   `gh workflow run container.yml --ref main -f tag=vX.Y.Z -f publish_latest=true`，
   不要传 `source_ref`），等待它通过，确认两个 GHCR package 已公开，分别核验
   版本与 digest，再匿名拉取两个完整版本标签。

应把已发布的资产和 tag 视为不可变。已发布 payload 有误时发新的 patch 版本，
不要替换资产或移动 tag。

## 发版前检查清单

推送 `v*` tag **前** 跑完这些检查。CI 冒烟覆盖大部分；需要真实桌面的部分手
动验证。

- [ ] 可复用质量门中的三个 job 全绿（含 `contract:v3:check`）；tag-only 签名
      `release:check` 通过；选中的每个 `pnpm run build` 与平台冒烟全绿。
- [ ] `git diff --check` 干净；相对上一个 tag 的 diff 只含预期范围；四份代码
      版本清单、`compose.example.yaml` 与 Cargo.lock 四个本地包条目一致。
- [ ] 每个 runner 的 `release/SHA256SUMS` 与目录内全部 payload 一致；
      `verify-release` 接受与组装产物逐名一致的附件集合、升级 manifest、四份
      签名、checksum 和 GitHub 服务端 digest。
- [ ] 跑 `cargo test -p ocg-core gemini` 与
      `cargo test -p ocg-core claude_desktop`；用 Bearer、`x-api-key`、
      `x-goog-api-key` 分别请求 Gemini `generateContent` 与
      `streamGenerateContent`，覆盖 Chat 原生与 Messages 原生模型，确认错误
      envelope、usage envelope、HTTP 状态和 SSE 终止行为符合客户端协议。确认
      `countTokens` / `embedContent` 返回 `501`，未知 action 返回 `404`。
- [ ] 确认非空 Gemini `safetySettings` 返回 `400`，`null` 与 `[]` 仍接受。用
      代表性的 `cachedContent`、`fileData`、Google Search、`urlContext` 请求验
      证它们在任何上游计费前失败。对 `topK`、`thinkingConfig` 只验证兼容可用，
      不在冒烟中断言与 Gemini 原生等价的语义。
- [ ] 验证带鉴权的 Claude Desktop 模型发现与 Messages 别名改写。通过
      `PUT /dashboard/api/v3/claude-desktop/models`（带 CAS 令牌）保存全部三个
      映射，用同一数据目录重启后确认映射仍在；非回环面板上确认无会话时映射 API
      返回 `401`。确认已退役 V2 `PUT /dashboard/api/claude-desktop/models` 在
      已鉴权时为 `410`。
- [ ] 打开 **应用** 视图，确认 16 个教程完整可选；逐项抽查复制结果不含掩码
      Key，并实际启动 Claude Desktop 与 Gemini CLI 各完成一次文本和工具调用。
- [ ] 覆盖 schema v16 迁移、schema v27（`access_keys`、pre-v3 备份 + SHA-256
      sidecar、删除 `sub_gateway_keys` 与 `accounts.usage_sync_*`、密文只校验
      不重加密）、存在时的旧 pre-v22/pre-v23 回滚副本、别名 / 上游日志身份、
      可选原生成本、未 `verified` 的 GOAT 行保持禁用 `pending`、Zen Free 模型
      快照持久化、供应商合约范围 / 模型协议表、旧账号 `key + ready`、托管状态
      机（前进一格 / 回退更早步骤、禁止跳步）、Pending 路由隔离、邀请 URL 白
      名单与演示默认写回，以及 Key 验证的 `2xx`/`429`/`401`/`403`/网络/`5xx`
      分支；确认除会话保护的 `GET /dashboard/api/v3/connection` 外，任何 DTO
      和日志都没有明文 Key。
- [ ] 确认带鉴权的 `GET /v1/models` 与受保护的
      `GET /dashboard/api/v3/application-models` 是本地读取，GET 本身不访问
      上游。`/v1/models` 是当前可路由已公布别名加上合格 Custom ID；
      `application-models` 是 Go 可路由别名 ∩ 当前价格快照（highspeed 继承基价
      行），不得包含 Custom。未知模型在 Chat / Responses / Messages / Gemini
      上返回 `400`，除非命中该 `/v1/models` 列表。Command Code GOAT / SCNet
      Token Plan 草稿保持禁用、不可路由（`routable=false`）、验证 `501`。不要
      把 GOAT 或 SCNet 当作已上线路由、用量、计价或供应商教程做冒烟。这些本地
      列表与 fail-closed 检查不需要真实供应商 Key。
- [ ] 有界假上游 Custom API 冒烟（不需要真实供应商 Key）：拒绝 URL 内嵌凭据；
      `2xx` JSON object 验证成功；账号仍保持禁用；必须显式启用；声明的模型/协议
      可转发；拒绝重定向；不转发 dashboard/client 鉴权，只发送已配置的 Bearer 或
      `x-api-key`；成功日志为 unpriced/`cost_state=unknown` 且不扣额度；编辑 URL、
      Key 或能力会使验证失效并禁用账号。确认 Direct/Manual/Auto 继承进程级代理。
- [ ] 在 Windows 验证 Edge/Chrome 优先级，在 macOS/Linux 验证浏览器发现；用两个
      账号确认 Profile 隔离和重启后 Cookie 保留。确认重置会退出控制台但保留完成
      账号 Key，删除会同时清理新旧 Profile，旧 WebView Profile 不会被导入。
- [ ] 人工完成（可跳过）登录身份 → 邀请链接 → OpenCode 登录 → 支付前确认页 →
      Key 回填；真实支付只由测试者明确执行。控制台打开 `opencode.ai/auth`。旧
      Key 账号首次打开控制台后登录一次，再验证实际额度和邀请使用情况可回访。
      已完成 Key 账号与托管账号都要验证 **刷新额度**（官方 `/zen/go/v1/usage`：
      无效 Key、换 Key 后 409、网络/schema 失败须明确报错且保留上次本地校准）。
      分别覆盖桌面和 Docker Sidecar。
- [ ] Windows 上本地跑一次安装包，确认 SmartScreen 警告文案，打开面板、添加
      账号、发一条请求。
- [ ] macOS 上挂载 DMG，确认 **Open Anyway** 流程可用，打开面板、添加账号、
      发一条请求。
- [ ] Linux 上装 `.deb`、跑 AppImage，CI 上 Xvfb 跑通，本地 Wayland 或 X11 真
      实会话里再确认一遍。
- [ ] Windows 上验证 `auto_start` 开关能切换 `HKCU\...\Run\OCG Manager`，且卸
      载后清理。
- [ ] 确认 `scripts/release.mjs` 报告原子替换 `release/` 成功，旧 `release/`
      已清掉。
- [ ] 本地构建两个容器，并在隔离卷上确认 UID/GID `10001`、内置 `LICENSE`、只读/
      capability 加固、面板鉴权和备份恢复后的属主权限。用
      `docker compose --profile browser up -d` 验证单 Chromium、noVNC 键鼠/剪贴板、
      账号切换、Sidecar 重启、1 GiB shm、无公开端口和双卷备份恢复。
- [ ] 推送 tag 前复核计划使用的 GitHub Release 说明与未签名 / ad-hoc 警告；公
      开后确认同一份说明和精确的已验证资产集合已经发布。
- [ ] 发布后确认 `container.yml` 通过，并按预期 digest 匿名拉取主镜像与
      `ghcr.io/klarkxy/opencode-go-mgr-browser:<version>`，再分别验证 signer
      workflow、SBOM 与 SLSA provenance；GitHub Release 仍为与组装产物逐名一致的
      附件集合。

## 已知缺口

- 服务端的 409 错误码是 `revisionConflict`，但 `src/api/dashboard-v3.ts` 仍检查
  `revision_conflict`，对应前端测试也模拟了旧拼写。因此真实冲突不会触发预期的
  token / 资源刷新；修复前需要用户手动刷新并重新提交。
- `dashboard.rs` 仍包含墓碑后面的已退役 V2 REST 处理器，以及仍然存活的 V2
  鉴权与 V2 浏览器 WebSocket。不要把这些处理器当成现行面板契约。新的 JSON
  属于 `dashboard_v3`。
- 部分前端单测仍从 `src/api/tauri.ts` 导入历史类型。生产页面使用
  `dashboard-v3.ts` / `dashboard.ts`。不要新增 `invoke()`；不要把 `tauri.ts`
  写成现行客户端。
- `auto_start` 受能力门控：只有 Windows release / 已安装的 Tauri 进程注入注册
  表同步钩子。开发构建、CLI、Docker、macOS、Linux 面板不暴露该开关。Dock
  可见性仅 macOS Tauri。
- 生成的 Tauri schema 文件会让 diff 变吵；除非 Tauri 配置真的改了，否则不要动
  它们。
- 流式用量仅在上游发出 usage chunk 时精确；Chat 流式请求会设置
  `stream_options.include_usage`。没有 chunk 时 Go 行记为 `success_no_usage`；
  Zen 无 usage 的成功仍为 `success` / `free`。
- 旧 `profiles/<account_id>` WebView Profile 不会迁移到外部 Chromium；升级后首次
  需要重新登录。保留旧路径仅用于重置/删除时安全清理，不能尝试跨引擎直接复用。
- Responses 端点是无状态。`previous_response_id`、`conversation`、
  `store: true`、`background: true` 直接返回 `400`，不会静默忽略。这是有意为
  之，详见 `protocol.rs` 和用户指南。
- Gemini 是客户端兼容格式，不是新的上游协议。仅实现 generateContent 文本、内联
  图片、函数调用、单候选 TEXT/JSON Schema 与 SSE 转换；没有 Google Search、URL
  Context、Code Execution、cached content、Gemini embeddings 或服务端 token 计
  数。非空 `safetySettings` 明确拒绝；`topK`、`thinkingConfig` 这两个兼容提示
  不保证在 Chat/Messages 上游保持等价行为；其他非空 `generationConfig` 字段必
  须明确映射或返回 `400`，不得静默丢弃。
- Claude Desktop 只公布三个固定 Claude 别名，再映射到受支持的实际模型；它不代
  表 OCG Manager 提供了原生 Claude 4.6 模型或完整 Anthropic Models API。
- Command Code GOAT 与 SCNet Token Plans 只是 schema/UI 草稿。它们创建禁用的
  `pending` 账号；验证为 `501`；不会被 Alias 路由选中。SCNet 官方可用模型表与
  endpoint 快照只作适配器输入，不得作为客户端别名公布。不要把这些家族写成或发成
  已上线支持。Custom API 已在受信管理员边界下上线（`custom.rs` +
  `custom_http.rs`）；不要把这条路径写进 GOAT/SCNet 防滥用口径。
- Custom 的供应商范围协议探测不在 V3；V2 账号侧探测路径已 410。Custom 验证与
  模型发现是现行操作路径。
- `console_usage.rs` 保持冻结。当前 V3 实现不要调用、扩展或删除它。
- 曾运行多 Key 开发构建（PR #43 config 内嵌形态，从未发布）的数据库：
  `/logs/forward/keys` 可能出现两个同名 "Primary"（旧随机 UUID 与
  `PRIMARY_KEY_ID`）。属可接受开发期残留；洁癖可删除 `data.sqlite` 重建。首次
  启动照常把 NULL 历史行回填到 `PRIMARY_KEY_ID`。

## 明确非目标

- 动态 / 插件式供应商扩展、用户自定义适配器，或持有 SQLite、`CoreState`、
  原始 `reqwest::Client` 的适配器。
- 远端节点同步、Admin API 或多租户控制面。
- 把 Tauri `invoke` 当作面板数据路径；WebView command 保持移除。
- 在 `GET /v1/models` 或 `GET /dashboard/api/v3/application-models` 上做请求时
  上游发现。
- 已上线的 GOAT 或 SCNet 路由、用量、计价、验证或供应商教程。
- `/embeddings`、Gemini `embedContent`（501），或把 Gemini `countTokens` 做成
  真实上游计数（501 供 Gemini CLI 回退本地估算）。
- 把 Gemini 当成上游协议。
- 自动轮询价格或 Zen 目录。
- 跨引擎复用旧 WebView Profile。
- 数据库降级，或让旧二进制打开更新 schema。
- Windows/Linux ARM64、32 位 x86、RPM、Snap、应用商店包、Windows
  Authenticode 或 Apple 公证。
- 在 GitHub provenance 之外再加一份 Cosign 镜像签名。

## 编码约定

- **Ponytail 原则**：能删就删，能复用现有代码就复用。代码库偏向扁平调用点，
  不要为想象中的需求加抽象——但不得因此省略必需的 CAS、墓碑或 fail-closed
  检查。
- **保持 crate DAG**。domain 与 gateway 保持无 I/O。门面按条目再导出。适配器
  返回 `AttemptSpec`。`forward_once` 是一次上游调用。Dashboard V3 不导入
  `gateway`。
- **不要新增前端 Tauri `invoke()` 路径**。Vue 主数据路径是 HTTP
  `/dashboard/api/v3`。不要注册 `generate_handler`。
- **不要复活受保护的 V2 REST**。新 JSON 属于 V3。410 墓碑必须挡在已退役
  `/dashboard/api/...` 路径前面。
- **不要削弱安全边界**。Gateway 鉴权、Key 混淆、URL 校验、冷却写入、SSE 透
  传以及 ConnectionInfo 密钥边界都不能为了简化拿掉。
- **不要重新引入远端同步**。每个节点由自己的面板管理。
- **`auto_start` 与 `show_dock_icon` 受能力门控**。只有 Windows release / 已
  安装的 Tauri 进程注入注册表同步钩子；Dock 仅 macOS Tauri。
- **本地 Alias 列表保持本地**。带鉴权的 `GET /v1/models` 与面板
  `application-models` 不得增加请求时上游发现。供应商页上的显式 Zen Free 刷新
  是唯一目录抓取例外，且只能访问固定官方 endpoint。不要把两份列表写成同一份；
  不要发明 `requested_alias` 日志字段。
- **不要重新发明 `cargo test` 体验**。CLI 与 core 用 `parking_lot::Mutex`，
  不可重入。函数需要调用另一个持锁函数时，先 `drop` 掉外层 guard。
- **风格与周围一致**。修改某段代码时，新代码要像旧代码：注释密度、命名风格、
  惯用法保持一致。

---

[English maintainer guide](MAINTAINER.md) · [User guide](USER.md) ·
[用户指南](USER.zh-CN.md) · [文档索引](README.md) ·
[v27 恢复](MAINTAINER-v3-migration.zh-CN.md) ·
[回到 README](../README.zh-CN.md)
