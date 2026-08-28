[English](runtime-invariants.md)

# 运行时不变式

运行系统的行为不变式。修改 Gateway 路由、别名、Zen Free、套餐目录、访问 Key、出站代理或用量同步前请先阅读本文。代码是最终权威；本页梳理容易出错的语义。

## Gateway 与模型列表

- Core Gateway：Axum + Tokio + reqwest。同一端口暴露 OpenAI Chat Completions / Responses、Anthropic Messages、Gemini `generateContent` 客户端入口，以及 Claude Desktop 别名入口。
- 已认证的 `GET /v1/models` 首先列出当前可路由的已发布别名（OpenCode Go 与上次成功保存的 Zen Free 模型快照），然后合并符合条件的 Custom 账号声明的模型 ID（enabled+ready+非空 Key；验证为可选）；受保护的 `GET /dashboard/api/v3/application-models` 仍是 **Go 可路由别名 ∩ 当前定价快照**（highspeed 继承 base-price 行；空交集为 `[]`），不含 Custom。两条 GET 路径都不会请求上游；只有当管理员在 Providers 页点击 Zen Free “获取模型” (Fetch Models) 时，才会命中固定官方目录。Custom ID 来自符合条件账号的声明能力。未知模型名（既不是已发布别名，也不是符合条件的 Custom ID）在所有支持的客户端格式上返回 `400`。
- Gemini 客户端使用 `/v1beta/models/{model}:generateContent` 或 `:streamGenerateContent`（`/v1/models/...` 也接受），可以用 `x-goog-api-key` 认证；Gemini 只是一种客户端格式，Gateway 总是把请求转换成目标模型的推荐上游协议。未知模型名在 Chat / Responses / Messages / Gemini 上返回 `400`；禁止探测协议。
- 模型协议能力硬编码在 `ocg_domain::protocol` 的 `MODEL_PROTOCOLS` 中（`ocg-core` 的 `kernel/protocol.rs` 与 `gateway/protocol.rs` 是 facade/host 转换）：`preferred` 与官方 Go 文档端点表一致，`supported` 来自测试账号探测结论。当客户端协议 ∈ supported 时直接透传，否则路由到 preferred；请求路径不得探测协议（避免重复计费）。`grok-4.5` 的 `supported` 只有 Responses（Chat 入口必须转换）。`gpt-5.6-luna` 的 preferred 仍是 Responses，但 Chat 现在可以透传。`MODEL_PROTOCOLS` 目前仍只服务于 OpenCode Go；Zen Free 刷新得到的新 `-free` ID 若表中未知，默认物化为 Chat，且不会使用计费请求探测协议。整篇 JSON 转换内核在 `ocg-gateway` 中。

## Dashboard V3 与 V2 墓碑

- Dashboard V3 挂载在 `/dashboard/api/v3`。控制面变更需要 CAS（`expectedRevision`，以及 `processGeneration`；价格写入还需要 `expectedPricingRevision`）。`ConnectionInfo` 是唯一允许返回明文 Key 的 V3 响应 DTO；Key 变更响应不包含明文，客户端会重新 `GET /connection`。
- `GET /contract` 返回当前进程的 live revision / generation token，不是契约导出端点。
- 旧版受保护 V2 REST（`/dashboard/api/...`，不含 V3）向已授权面板会话返回结构化 `410`（`code=dashboardV2Removed`）；匿名请求先返回 `401`。以下语义与墓碑相互独立，不得混淆：V2/V3 认证与会话、browser WebSocket、推理入口。唯一保留的无版本路径是精确的 `auth/status|register|login|logout` 与 `browser/sessions/{token}/ws`。
- `crates/ocg-core/src/dashboard.rs` 现在只处理 SPA `index`/`assets` 与保留的 V2 认证、browser WS 处理器；已退役 V2 REST 的墓碑中间件位于 `host_router.rs`。Go/Zen 协议探测在 V3 路径 `POST /providers/{provider_id}/protocol-probes`。Custom 账号级协议探测没有 V3 对应端点；已退役 V2 account 路径授权后返回 `410`。不要复活 V2 REST 来“补全探测”。该端点拒绝不在 effective 供应商目录内的模型，也不提供账号选择。对于每个通过目录校验的协议，后端按已保存顺序遍历符合路由资格的账号，首次成功即停止；静态协议表尚未收录的新拉取模型也遵循此流程。每个真实账号/协议尝试都会写入一条脱敏的 `forward_logs` 请求日志，任何请求相关内容都不会写入 `gateway_logs`。聚合成功会持久化正向证据并写入 `force_on`；失败尝试只保留失败证据，绝不会创建供应商级 `force_off`，只有显式覆盖端点可以固定 `force_off`。探测失败的观测本身绝不会降级静态/预设支持。
- 请求可观测性只属于 `forward_logs`：转发尝试、供应商协议探测，以及已认证的本地解析/校验/路由失败都不得重复写入 `gateway_logs`。账号选择前失败使用“未解析/Gateway”归因，token 字段为零且 `cost_state=not_applicable`。`gateway_logs` 只保留运行生命周期、控制面事件，以及请求日志行持久化或收尾失败。未认证请求与预期的本地 fallback 仍不持久化。

## 访问 Key 与认证

- 从 schema v31 起，权威表是 SQLite `access_keys`（主 Key 固定 id `gateway_keys::PRIMARY_KEY_ID` / `00000000-0000-0000-0000-000000000001`，名称快照为 "Primary"，永不可禁用/删除；子 Key 是非主行，活跃上限 64，软删除保留名称但清空明文）。消毒后的配置 JSON 把 `gateway_key` 存为 `""`，不再是 DB 权威；进程内 `AppConfig.gateway_key` 与 `GET /dashboard/api/v3/connection` 仍暴露 live 主 Key。生命周期只能通过 `/dashboard/api/v3/keys*`（包括 `POST /keys/primary/regenerate`）。主/子 Key 值互斥由 `gateway_keys::ensure_primary_value_allowed` 强制执行。`sub_gateway_keys` 只出现在迁移到 v27 之前的历史库中，迁移后即丢弃；不要把它描述为当前权威表。
- 认证收集所有非空候选头 Bearer / x-api-key / x-goog-api-key；任一匹配凭据快照（`CoreStateInner.credential_snapshot`，含主 Key 与已启用子 Key）即通过，归因按候选头顺序中的第一个匹配；同一快照也用于转发日志名称快照。
- 非 loopback 监听器使用单管理员登录。Docker 可通过 `OCG_ADMIN_USERNAME` 与 `OCG_ADMIN_PASSWORD` 首次初始化（两者必须同时设置；只设一个会导致启动错误）；未提供时，首个注册用户成为管理员。

## 持久化

- SQLite，当前 schema **v32**。历史库在原地迁移；v27 把主 Key 与 `sub_gateway_keys` 复制进 `access_keys`，并删除 `accounts` 上遗留的五个 `usage_sync_*` 列。v29 移除 SCNet Token Plans，v30 引入过渡期 Custom 多协议 JSON 集合，v31 新增按模型/按协议覆盖表。v32 将每条 Custom 配置替换为 `endpoint_url` 与单值 `upstream_protocol`；历史行按 Chat → Responses → Messages 选择协议并拼出标准推理路径，清理非所选协议的能力/证据/覆盖，同时把账号设为 disabled/pending。已有非空库在跨越主要 schema 边界前生成不覆盖的 `data.sqlite.pre-v3.<UTC>.bak` 与 `.sha256`；全新空库直接创建当前 schema。GUI 数据目录在 Windows 为 `%USERPROFILE%\.ocg-mgr`，macOS/Linux 为 `~/.ocg-mgr`；CLI 默认为 `~/.ocg-mgr-cli`。升级与回滚详见 `docs/maintainer/storage-migration.zh-CN.md`。
- 下游访问根 URL 优先级：非空 `OCG_CLIENT_ROOT_URL` > SQLite 手动值 > 前端从生产 origin / 开发 Gateway 端口自动推导。环境变量覆盖是只读的，不得写回 SQLite。

## 桌面端宿主

- Tauri v2 跨平台托盘应用；主窗口默认隐藏；托盘/单实例逻辑打开 `http://127.0.0.1:<port>/dashboard/`，loopback 监听器自动跳过登录。宿主能力（Gateway 生命周期、原生浏览器、自动启动、Dock、升级器）注册进 `CoreState`，**不会**注册为 `#[tauri::command]` / `invoke_handler`。不要描述“仍有 live Tauri invoke 命令”为当前状态。
- Settings 页通过受保护 `GET /dashboard/api/v3/settings/check-update` 手动检查最新 GitHub Release。内置了升级器公钥的已安装桌面版可下载、校验签名并原地安装；开发版、CLI、Docker 以及尚未进入更新通道的旧版本仍走 release 页面 / 手动覆盖路径。

## 出站代理

- 全局出站代理存储在 `AppConfig` 中，模式包括 auto（系统/环境）、manual HTTP、force-direct，以及按模型列表（List）。非 List 模式三者互斥；List 模式（`proxy_list_direction` allow/deny list + `proxy_list_models` 已知模型 id）中，列入模型的走方向例外段（allowlist → proxy / denylist → direct），未列入模型与非模型出站（账号测试/验证、Zen Free 手动模型刷新、用量、定价、升级器下载）走方向默认段（allowlist → direct / denylist → proxy）。列表成员校验只在面板 `PUT /dashboard/api/v3/settings` 写入关卡运行（非空、精确已知 id、去重）；加载路径容忍过期值。Zen Free 仅在管理员显式刷新时命中固定 `https://opencode.ai/zen/v1/models`，无需 Key、不跟随重定向，刷新失败或返回空时保留旧快照。reqwest 路径经 `ocg-core` 的 `http_client.rs` facade 进入 `ocg_infra::http` 的 route set / `configured_builder`；Tauri 升级器使用其 `proxy` / `no_proxy` 以与默认段保持一致，不得绕过按账号配置。转发从请求入口快照选取路由；热配置切换不影响飞行中请求。Custom HTTP（`custom.rs` + `custom_http.rs`，传输可能复用 `ocg_infra::inference_http`）遵循同一代理策略；永不跟随重定向；永不转发面板/客户端认证；从唯一协议自动选择一个鉴权头（Chat/Responses 为 Bearer，Messages 为 `x-api-key`）；超时由 `connect_timeout_secs` 限制在 5–60 秒。

## 套餐目录与 Custom API 边界

- 套餐目录位于静态 `ocg_domain::provider` 的 `BUILTIN_PLANS`：OpenCode Go、Zen Free、Command Code GOAT 与 Custom API。内部身份是 `provider_id` + `offering_id`。Command Code GOAT 可路由；其官方公开 `GET /models` 是供应商目录发现，不是对已保存 Key 的验证。历史 GOAT 验证状态统一为 `not_required`，真实 Key 鉴权边界仍是推理请求返回的 401/403。账号只贡献 enabled+ready+非空 Key 的凭据与顺序，模型供应由供应商模型/协议合约控制：GOAT 内含预设默认开启，额外发现的模型默认关闭，必须显式 `force_on`；不再有账号级 GOAT/全部或 Max 权限模式。已验证的 GOAT 价格快照提供逐请求的价格与倍率核算，并绝不回退套用 Go 价格。Command Code 没有可机读的账号用量端点，因此 GOAT 的用量可用性为 `local_state`，绝不是权威值：面板把 OCG 内已定价请求日志投影到公开的 `$14 / $35 / $70` 三个窗口，并允许手工修正基线。未定价日志与 OCG 外流量不会计入；这套估算不影响推理资格，也不会启动自动同步。所有持久化变更路径仍会在写入、revision 或 timestamp 变更前拒绝启用真正 `routable=false` 的套餐；桌面 UI 只通过 Dashboard V3 HTTP 变更，没有单独的 invoke 变更路径。
- Custom API（`custom`/`api`，`routable=true`）是可信管理员目标：每张账号卡配置一个完整 HTTP/HTTPS 推理 Endpoint 和一个 `chat_completions` / `responses` / `messages` 协议。拒绝内嵌凭据、query、fragment 与重定向；不转发面板/客户端认证。Chat/Responses 只构造 Bearer，Messages 只构造 `x-api-key`，不存在鉴权覆盖或换头重试；保存的 Endpoint 原样使用。唯一协议对全部声明模型统一生效，也是 effective preferred protocol：同协议请求透传，其他受支持客户端格式（包括 Gemini）转换到它。Custom 配置与完整模型能力列表通过一次 V3 CAS 原子替换。验证可选，只向完整 Endpoint 发送一次所选协议的最小请求。模型发现仅从所选协议的标准后缀推导 `/models`；非标准路径明确失败并保留手工模型。Custom 覆盖只能指向账号声明协议，`force_on` 不能扩张到未声明协议；`force_off` 后模型不可路由且没有固定顺序回退。符合条件的账号动态路由声明模型 ID。Custom 成本/用量未定价/未知。多端点供应商应以后新增静态 Provider Adapter，而不是扩张通用 Custom 路由。

## 别名

- 客户端别名位于 `ocg_gateway::alias`（`ocg-core` 的 `alias.rs` 是兼容 facade）：首选稳定小写 kebab-case，并以 Go 模型 ID 为基准；Command Code 条目能匹配时加入同一 canonical Alias。大小写折叠可接受；包含 `/`、`_` 或空格的 raw ID 不得做分隔符折叠。raw ID 若恰好只有一条注册表映射，则在检查可路由性前先固定到该映射；不可路由的映射仍被识别，但无法产生生产路由。重叠 raw ID（包括符合条件的 Custom 声明 ID 与另一套餐映射冲突）返回 `ambiguous_model_id`，不会调用上游。Zen Free 只发布去掉后缀的 Alias `foo`，原始 `foo-free` 仍是精确 raw pin；共享 Alias 按账号卡片持久化顺序在 Go/Zen/Command 候选者中选择。符合条件的 Custom 声明 ID 叠加进解析与 `/v1/models`，但不得抢占已发布 Alias。`/v1/models` 只在至少一个映射仍有启用的 effective 协议时发布该 Alias；供应商全关后不产生路由。转发日志区分 `requested_model`、`resolved_alias` 与 `upstream_model`；`native_cost_*` 为可选；不要臆造 `requested_alias` 字段。Claude Desktop 的三个角色别名仍在别名解析前被重写；`/claude-desktop/v1/models` 只发布这三个角色。

## Zen Free

- Zen Free 是特殊的内置账号，没有 Key；只有账号卡片启用开关，不再有 `deny` / `explicit` / `prefer` 或自动 prefer 策略。管理员在 Providers 页点击“获取模型” (Fetch Models) 时，请求固定官方目录，仅保留以 `-free` 结尾的规范化有效 ID，持久化上次成功快照，并生成去掉后缀的别名；刷新失败或空结果不会覆盖旧快照。不需要 Free 时关闭卡片；启用时按卡片顺序与其他账号一起被选择。协议探测控件也在 Providers 页，而不是账号卡片。Zen Free 与 Go 使用独立的 `cooldown_free_until`；Zen Free 配额按出口 IP 共享，收到 429 后整个 Free 通道冷却，不切换 Key，路由继续尝试后续兼容卡片，仅当只剩 Free 候选者时才返回共享冷却。推理 `401` 原样返回客户端，不切换凭据也不写入 `auth_error`；面板 Ping / Key 验证的 401 仍记录 `auth_error`。Free 通道成功行记录 `cost_state=free`，不计入 Go 配额。Go 的 `ox-alpha-free` 仍由 Go 静态映射处理，计为 `unpriced`，不算 Free。

## Claude桌面

- Claude Desktop 使用 `/claude-desktop/v1/messages` 与 `/claude-desktop/v1/models`；`sonnet`、`opus`、`haiku` 映射存储在 `AppConfig.claude_desktop_models` 中，由受保护 `GET/PUT /dashboard/api/v3/claude-desktop/models` 管理。

## 托管账号（Beta）

- `setup_step` 顺序为 `google_account`（UI：登录身份，可跳过）→ `opencode_registration` → `payment` → `key_verification` → `ready`。`PATCH /dashboard/api/v3/accounts/{id}/setup` 允许前进一步或回退到更早步骤；禁止跳过步骤或直达 `ready`。草稿创建可编辑邀请链接并写回 `opencode_invite_url`（`DEFAULT_OPENCODE_INVITE_URL` 是演示默认值）。浏览器目标包括 Google/GitHub 注册与登录、邀请 URL，以及控制台 `https://opencode.ai/auth`。托管页可通过 dashboard HTTP 打开浏览器；桌面原生浏览器是 Host hook，不是 WebView invoke。

## 用量同步

- 已完成账号的配额：官方 `https://opencode.ai/zen/go/v1/usage`（`go_usage.rs`）是周期性校准基线；本地 `forward_logs` 在上次成功校准后仍做实时估算。`usage_sync.rs` 协调手动与后台路径：ready+enabled 且最近约 24h 内有本地活动的账号约每小时对账一次，不活跃的约每天一次；disabled/not-ready/空 Key 账号不自动刷新。全局并发 1，带 jitter 与可注入 clock/jitter/fetch seams；无启动惊群。手动 `POST /dashboard/api/v3/accounts/{id}/usage/refresh` 仍可用；服务端限流为每账号 15s（无论成功失败都计入），带并发去重，返回 Retry-After / `next_allowed_at`；失败保留上次基线与上次成功。本地最大 Go 用量 ≥80% 时，加速对账至多每 15 分钟一次。真实推理 429 仍写入现有冷却/选择器，并额外安排约 1–2 分钟后官方对账（非内联）；官方失败或 `status=rate-limited` 从不写入推理冷却。成功后按最早 `resetsAt`（加有界 jitter）重新调度，尊重活跃/不活跃节奏。失败退避：5m → 15m → 1h → 6h。同步元数据位于 `provider_usage_sync_state`（`accounts.usage_sync_*` 不再使用）。共享实现包括 CAS / 三窗原子校准与全局代理。官方 Go 文档未列出该端点。不要引入 CDP 自动化刷新。

## 定价、容器与 CI 说明

- 定价按 Provider 独立管理：读取 `GET /dashboard/api/v3/providers/{provider_id}/{offering_id}/pricing`，刷新 `POST /dashboard/api/v3/providers/{provider_id}/pricing/refresh`；Go 倍率写入使用 `PUT /dashboard/api/v3/providers/opencode/go/pricing/multipliers`。OpenCode 与 Command Code 各自维护修订号和最后一次成功快照；只有用户点击对应 Provider 的刷新按钮时才访问其固定官方来源，禁止自动轮询。
- 公开 GitHub Release 发布后，`.github/workflows/container.yml` 在原生 amd64（`ubuntu-24.04`）与 arm64（`ubuntu-24.04-arm`）runner 上构建 `linux/amd64` 与 `linux/arm64` 镜像（仅 amd64 跑冒烟测试），按 digest 推送各架构，再合并为同一标签下的多架构 OCI index，发布到 `ghcr.io/klarkxy/opencode-go-mgr`。Compose 默认使用该镜像；本地源码构建需 `OCG_IMAGE=ocg-manager:local` 后 `docker compose up -d --build`。
- `.github/workflows/quality.yml` 在 PR / `main` 上分为三个并行 job：Web（含 `pnpm run contract:v3:check`、前端测试/类型/lint）、Linux workspace Rust 测试/Clippy（排除 Tauri 桌面 crate，无需安装系统包）与 Windows Tauri 目标测试（stub `dist/`，不运行 Vite）。`release.yml` 仅在生产 `v*` tag push 时调用该质量门。`release.yml` 手动候选（即使选择 tag ref）始终未签名，且可能只构建所选平台；只有 `v*` tag push 事件才会构建全部三个平台并读取仓库签名密钥。tag push 被视为单维护者显式发版授权：工作流逐个校验附件集合与组装产物名称匹配（数量由产物推导，非硬编码）、升级器签名、公钥连续性，以及 GitHub 服务端摘要，然后自动发布同一未改动草稿。
- 容器以固定 UID/GID `10001` 运行，包含 `LICENSE`；Compose 透传可选 `OCG_MANAGER_ENCRYPTION_KEY` 以支持显式 Key 恢复，但正常部署仍倾向于在卷中保留 `.encryption-key`。

---

[维护者指南索引](../MAINTAINER.zh-CN.md) · [English](runtime-invariants.md) · [文档索引](../README.zh-CN.md)
