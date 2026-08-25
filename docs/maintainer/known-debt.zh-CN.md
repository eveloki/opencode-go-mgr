[English](known-debt.md)

# 已知缺口与明确非目标

## 已知缺口

- 服务端的 409 错误码是 `revisionConflict`，但 `src/api/dashboard-v3.ts` 仍检查 `revision_conflict`，对应前端测试也模拟了旧拼写。因此真实冲突不会触发预期的 token / 资源刷新；修复前需要用户手动刷新并重新提交。
- `auto_start` 受能力门控：只有 Windows release / 已安装的 Tauri 进程注入注册表同步钩子。开发构建、CLI、Docker、macOS、Linux 面板不暴露该开关。Dock 可见性仅 macOS Tauri。
- 生成的 Tauri schema 文件会让 diff 变吵；只在 Tauri 配置确实改动时才需要修改它们。
- 流式用量仅在上游发出 usage chunk 时精确；Chat 流式请求会设置 `stream_options.include_usage`。没有 chunk 时 Go 行记为 `success_no_usage`； Zen 无 usage 的成功仍为 `success` / `free`。
- 旧 `profiles/<account_id>` WebView Profile 不会迁移到外部 Chromium；升级后首次需要重新登录。旧路径只保留用于重置/删除时的安全清理，跨引擎无法直接复用。
- Responses 端点是无状态。`previous_response_id`、`conversation`、 `store: true`、`background: true` 直接返回 `400`，不会静默忽略。这是有意为之，详见 `protocol.rs` 和[用户指南](../USER.zh-CN.md)。
- Gemini 是客户端兼容格式，不是新的上游协议。仅实现 generateContent 文本、内联图片、函数调用、单候选 TEXT/JSON Schema 与 SSE 转换；没有 Google Search、URL Context、Code Execution、cached content、Gemini embeddings 或服务端 token 计数。非空 `safetySettings` 明确拒绝；`topK`、`thinkingConfig` 这两个兼容提示不保证在 Chat/Messages 上游保持等价行为；其他非空 `generationConfig` 字段必须明确映射或返回 `400`，不会静默丢弃。
- Claude Desktop 只公布三个固定 Claude 别名，再映射到受支持的实际模型；它不代表 OCG Manager 提供了原生 Claude 4.6 模型或完整 Anthropic Models API。
- Command Code GOAT 与 SCNet Token Plans 只是 schema/UI 草稿。它们创建禁用的 `pending` 账号；验证为 `501`；Alias 路由不会选中它们。SCNet 官方可用模型表与 endpoint 快照只作适配器输入，不会作为客户端别名公布。这些家族不是已上线支持。Custom API 已在受信管理员边界下上线（`custom.rs` + `custom_http.rs`）；GOAT/SCNet 防滥用口径不包含这条路径。
- Custom 的供应商范围协议探测不在 V3；V2 账号侧探测路径已 410。Custom 验证与模型发现是现行操作路径。

## 明确非目标

- 动态 / 插件式供应商扩展、用户自定义适配器，或持有 SQLite、`CoreState`、原始 `reqwest::Client` 的适配器。
- 远端节点同步、Admin API 或多租户控制面。
- Tauri `invoke` 不是面板数据路径；WebView command 保持移除。
- 不会在 `GET /v1/models` 或 `GET /dashboard/api/v3/application-models` 上做请求时上游发现。
- 已上线的 GOAT 或 SCNet 路由、用量、计价、验证或供应商教程。
- `/embeddings`、Gemini `embedContent`（501），或把 Gemini `countTokens` 做成真实上游计数（501 供 Gemini CLI 回退本地估算）。
- Gemini 不作为上游协议使用。
- 自动轮询价格或 Zen 目录。
- 旧 WebView Profile 不会跨引擎复用。
- 数据库降级，或让旧二进制打开更新后的 schema。
- Windows/Linux ARM64、32 位 x86、RPM、Snap、应用商店包、Windows Authenticode 或 Apple 公证。
- 在 GitHub provenance 之外再加一份 Cosign 镜像签名。

---

[维护者指南索引](../MAINTAINER.zh-CN.md) · [English](known-debt.md) · [文档索引](../README.zh-CN.md)
