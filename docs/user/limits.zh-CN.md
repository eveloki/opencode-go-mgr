[English](limits.md)

# 限制

每个 Gateway 都会在某处画一条线。本页就是那条线——OCG Manager 拒绝做的事情列表，通常用 `400` 而不是善意的谎言来回答。

- 未实现 `/embeddings`。Gemini `embedContent` 会被路由，但返回 Google 风格的 `501 UNIMPLEMENTED`。
- Gemini `countTokens` 同样返回 `501`；Gemini CLI 预期回退到本地估算。只有 `generateContent` 与 `streamGenerateContent` 会真正转发。
- 非空 Gemini `safetySettings` 返回 `400`，因为不同上游协议无法保留其安全语义； `null` 与空数组不携带策略，可以接受。
- Gemini `cachedContent`、`fileData`、Google Search 工具、`urlContext`、Code Execution、多模态 function response、function response 的 schema/behavior、 `VALIDATED` 函数调用、`candidateCount` 大于 1、非 TEXT 输出模态返回 `400`。图片请改用 base64 `inlineData`，支持 PNG、JPEG、GIF、WebP。
- Gemini `topK` 与 `thinkingConfig` 只作为跨协议兼容提示接受；Chat Completions 或 Messages 原生上游可能忽略或实现不同语义，不保证与 Gemini 原生后端的采样和思考行为等价。
- 其他无法保留的非空生成选项（包括 `seed`、presence/frequency penalty、logprobs 与 media resolution）返回 `400`，不会静默丢弃。
- Responses 是无状态端点：必须设置 `store: false`。`previous_response_id`、 `conversation`、`store: true`、`background: true` 全部直接 `400` 拒绝，不会静默忽略。
- Responses 支持图片 URL 与 data URL；`input_image.file_id` 返回 `400`，因为 Gateway 没有 Files API。
- 跨协议转换无法保留约束的结构化输出与自定义工具语法会返回 `400`。
- Responses 的 `web_search`、`web_search_preview`、`tool_search` 等托管工具在 OpenCode-Go 上无法运行；自动工具模式下会被丢弃，显式强制使用则返回 `400`。 function、custom、namespace 工具正常转换。
- 流式 token 数量仅在上游发出 usage chunk 时准确；Chat 流式请求会设置 `stream_options.include_usage`。额度消耗使用当前 OpenCode Go 价格快照。没有 usage 时日志记为 `success_no_usage`。
- 浏览器向导只提供人工页面操作，不自动注册 Google、处理验证码、支付、抓取网页或提取 Key。
- 已安装的 Windows 桌面版可以在用户登录时把 OCG Manager 拉起到托盘；开发构建、 macOS、Linux、CLI、Docker 不暴露面板里的 `auto_start` 开关。Docker Compose 另由 `restart: unless-stopped` 在 Docker daemon 重启后恢复服务。
- macOS 桌面版可以在设置中隐藏 Dock 图标而只保留菜单栏图标；Windows、Linux、 CLI 与 Docker 不暴露 `show_dock_icon` 开关。
- 不发布 Windows / Linux ARM64、32 位 x86 构建；不支持 RPM、Snap、应用商店包、 Windows Authenticode 正式签名、Apple 公证。该口径仅覆盖桌面安装包；容器镜像（`ghcr.io/klarkxy/opencode-go-mgr` 及其 `-browser` 侧车）发布 `linux/amd64` 与 `linux/arm64`。支持升级的已安装桌面版可在设置页安装签名 Release；开发构建、CLI、Docker 使用直接/手动升级路径。
- Command Code GOAT 与 SCNet Token Plans 可以保存为禁用的 `pending` 草稿（`routable=false`）。连接验证返回 `501`；它们没有已上线的推理、用量、计价、验证运行时或生产路由，探测也不能把它们提升为生产路由。它们在 **供应商** 页显示为不可路由范围。Custom API 已在 [账号](accounts.zh-CN.md#账号) 的受信管理员边界下上线路由；不计价，也没有官方用量路径，其目录、协议与价格控制作为隔离的 `CustomEndpoint` 范围呈现在 **供应商** 页。
- 未知模型名在所有受支持的客户端格式上返回 `400`。客户端应发送带鉴权的 `GET /v1/models` 公布的、当前有有效启用协议的别名或合格 Custom ID。受保护的 `GET /dashboard/api/v3/application-models` 是 Go 别名 ∩ 当前价格快照，不是那份完整客户端列表。

---

[用户指南索引](../USER.zh-CN.md) · [English](limits.md) · [文档索引](../README.zh-CN.md)
