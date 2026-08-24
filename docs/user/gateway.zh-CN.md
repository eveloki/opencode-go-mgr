[English](gateway.md)

# Gateway 行为

## 端点

Gateway 监听 `http://<bind>:<port>`，暴露：

| 方法 | 路径 | 用途 |
| --- | --- | --- |
| `POST` | `/v1/chat/completions` | OpenAI Chat Completions |
| `POST` | `/v1/responses` | OpenAI Responses |
| `POST` | `/v1/messages` | Anthropic Messages |
| `GET`  | `/v1/models` | 带鉴权的本地列表：当前有有效启用协议的 Go 别名、已保存 Zen Free 目录与合格 Custom ID；GET 本身不访问上游 |
| `POST` | `/v1beta/models/{model}:generateContent` | Gemini 非流式生成；`/v1/...` 同样可用 |
| `POST` | `/v1beta/models/{model}:streamGenerateContent` | Gemini SSE 生成；`/v1/...` 同样可用 |
| `POST` | `/v1beta/models/{model}:countTokens` | 返回 `501`，Gemini CLI 可回退到本地估算 |
| `POST` | `/v1beta/models/{model}:embedContent` | 返回 `501`；当前不支持 embeddings |
| `GET`  | `/claude-desktop/v1/models` | Claude Desktop 可选别名列表 |
| `POST` | `/claude-desktop/v1/messages` | Claude Desktop Messages；改写三个 Claude 模型别名 |
| `GET`  | `/dashboard/` | Vue 3 管理面板（HTML） |
| `*`    | `/dashboard/api/v3/...` | 当前管理面板 JSON API |
| `*`    | `/dashboard/api/...` | 已退役的 V2 REST（已登录返回 410 `dashboardV2Removed`），不含已标明的 V2 鉴权与浏览器 WebSocket 兼容路由 |

默认监听 `127.0.0.1:9042`。CLI 可用 `serve --host 0.0.0.0` 覆盖监听地址，用 `serve --port <port>` 覆盖端口。桌面端同样绑定回环，并由 Tauri 单实例锁防止两个托盘程序争抢端口。项目没有 HTTP 健康检查端点；Docker 只检查容器内部的 TCP `9042` 端口。

## 鉴权

Gateway API 必须携带 **Key**，可使用 `Authorization: Bearer <key>`、 `x-api-key: <key>` 或 `x-goog-api-key: <key>` 三种请求头。转发前 Gateway 会移除客户端鉴权头，再注入所选账号凭据。OpenCode Go 在 Messages 上游使用 `x-api-key`，Chat Completions 与 Responses 上游使用 `Authorization: Bearer`。 Custom API 只构造已配置的 Bearer 或 `x-api-key`，不会转发 dashboard 或客户端凭据。

管理面板的鉴权模式取决于监听地址。当前 SPA 使用 `/dashboard/api/v3/auth/status`、`/dashboard/api/v3/auth/register`、 `/dashboard/api/v3/auth/login` 与 `/dashboard/api/v3/auth/logout`。注册、登录、退出需要与其他 V3 写入相同的 `expectedRevision` / `processGeneration` token。对应的 `/dashboard/api/auth/...` 路由只作为已标明的 V2 兼容例外，供缓存的旧页面使用，不是当前 SPA 数据路径。

- **回环监听（默认）**：直接发到回环地址的请求跳过面板登录；但只要带有 `Forwarded`、`x-forwarded-for`、`x-forwarded-proto` 或 `x-real-ip` 中任一请求头，仍必须登录。客户端还需要 **Key** 才能访问上游端点。桌面端与默认 CLI 都走这个分支。
- **非回环监听**：管理面板由唯一的 **管理员账号** 管控，密码以 Argon2 哈希存在 SQLite 中，登录后下发 HttpOnly 会话 Cookie。携带标准反向代理转发头但没有 Cookie 的请求仍需要登录。Docker 可以用 `OCG_ADMIN_USERNAME` 与 `OCG_ADMIN_PASSWORD` 引导首个管理员；不提供时由首位注册者创建。

## 别名

客户端应发送 **别名**：本地注册表中的稳定小写 kebab-case 名称。现有 OpenCode Go 模型 ID 就是首选别名；大小写折叠的拼写如 `GLM-5.2` 也可接受。

带鉴权的 `GET /v1/models` 先按注册表顺序列出当前可路由且有有效启用协议的已公布别名（OpenCode Go 与 Zen Free），再并入不与这些别名冲突、同样有有效启用协议的合格 Custom 能力 ID（`owned_by` 为 `custom`）。该列表 **不会** 发现、代理或缓存上游目录，也不会写转发日志或改路由状态。Zen 目录只在管理员于 **供应商** 页显式刷新时更新，本端点读取那份已保存快照。已公布的 Go 与 Zen Free 别名不依赖是否存在 Go 账号。合格 Custom ID 来自 enabled + verified + ready 且有 Key 的 Custom 账号。动态或探测确认的模型不会自动获得新的稳定别名。

受保护的 `GET /dashboard/api/v3/application-models` 是另一份本地列表：当前可路由的 OpenCode Go 别名与当前 OpenCode Go 价格快照求交。highspeed 变体继承基价行。空交集是 `[]`。它不含 Custom ID，也不选账号、不调用上游。

两份列表都不会公布 SCNet 官方可用模型表拼写或未公布的 Command Code GOAT 名称。合格 Custom 声明 ID 即使含 `/` 也可以出现在 `/v1/models` 上；它们不会折成 kebab 别名。

原始上游 ID 在注册表中恰好对应一个 mapping 时，会钉在该 mapping 上（不跨 Plan 回退，也不做 Zen prefer 覆盖）；之后才检查可路由性。因此不可路由 mapping 会被识别，但不能产出生产路由。名称里含 `/`、`_` 或空白时一律视为原始 ID，不会折叠成 kebab 别名（`glm/5.2` 不是 `glm-5.2`）。映射到多个 Plan 的原始 ID（含合格 Custom 能力与另一 Plan）返回 `400`，错误码 `ambiguous_model_id`，且不会调用上游。未知名称——既非已公布别名也非合格 Custom ID——在所有受支持的客户端格式上返回 `400`：Chat Completions、 Responses、Messages，以及 Gemini `generateContent` / `streamGenerateContent`。已公布的 kebab 别名 `deepseek-v4-flash` 仍归 Go；唯一原始 ID `deepseek/deepseek-v4-flash` 钉在 Command Code GOAT，不可作为生产路由，除非与合格 Custom ID 冲突而变为 `ambiguous_model_id`。

转发日志把请求身份与上游身份分开记录。没有 `requested_alias` 字段：

- `requested_model` — 客户端发送的别名或模型名
- `resolved_alias` — 存在时的规范 kebab 别名
- `upstream_model` — 该 Plan 的原始上游 ID

以及 `provider_id` 与 `offering_id`。原生成本字段可选。

Claude Desktop 仍是独立的三角色别名层（`claude-sonnet-4-6`、`claude-opus-4-6`、 `claude-haiku-4-5-20251001`），先改写为 **应用** 视图保存的映射，再进入 Alias 解析。`GET /claude-desktop/v1/models` 仍然只公布这三个角色别名，而不是 Plan 模型并集。

---

[用户指南索引](../USER.zh-CN.md) · [English](gateway.md) · [文档索引](../README.zh-CN.md)
