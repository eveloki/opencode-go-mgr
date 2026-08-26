[English](applications.md)

# 应用教程与模型能力

## 应用教程

**应用** 视图给 16 个客户端各自备好了一份配置片段——它们都觉得自己该有独享待遇。接入区展示当前客户端的请求地址、Key 选择器（主 Key 与已启用子 Key）和模型选择；节点地址与上游地址仍在仪表盘。每个教程列出协议、官方文档链接、操作步骤，以及一个或多个带 **复制** 按钮的可编辑代码块。屏幕上的代码块中 Key 已脱敏，复制出来的才是真实 Key，方便分享截图。

覆盖任何现有配置文件前，请先备份原文件。应用页的代码块可以编辑，但复制或手动合并之前都应保留一份可恢复的原始配置。

各客户端对 Base URL 都有自己的执念：

- Claude Code、Cherry Studio、Chatbox 使用不带 `/v1` 的根地址。
- Claude Desktop 使用根地址加 `/claude-desktop`，由客户端继续请求 `/claude-desktop/v1/messages` 与 `/claude-desktop/v1/models`。
- Gemini CLI 使用根地址，并设置 `GOOGLE_GENAI_API_VERSION=v1beta`。远端 Base URL 必须使用 HTTPS；只有 `localhost`、`127.0.0.1` 与 `[::1]` 可用 HTTP。解析出的根地址不符合该客户端限制时，应用页会禁用 Gemini 配置复制。
- Pi、Kimi Code CLI、OpenCode、OpenClaw、Hermes、Cline、Roo Code、Continue 使用带 `/v1` 的 API Base URL。
- VS Code Copilot Chat 与 WorkBuddy 使用完整 `/v1/chat/completions` 端点。Codex 使用带 `/v1` 的 API Base URL，且必须 `wire_api = "responses"`。CLI 用 `~/.codex/ocg.config.toml` + `codex --profile ocg`，Desktop 或常驻默认则合并进用户级 `~/.codex/config.toml`。`~/.codex/ocg-model-catalog.json` 可选：不写也能请求；只有需要选择器、真实上下文窗口和推理档位时才启用 `model_catalog_json`。启用后会整份替换 Codex 内置目录，且必须包含当前必填字段。不写 catalog 时，未知 slug 按 Codex 的 272K 回退元数据。请求始终走 OCG Manager 的 Responses 入口。

Codex 的 `~/.codex/ocg-model-catalog.json`、`~/.codex/ocg.config.toml` 和 `~/.codex/config.toml` 都属于本机配置。覆盖或合并前先分别备份；如果使用 CC Switch 的代理模式，请按 CC Switch 保存的配置目录单独备份，并把 OCG 直连配置和代理配置保存在各自独立的目录。

选择器列表来自受保护的 `GET /dashboard/api/v3/application-models`：当前可路由的 OpenCode Go 别名与当前价格快照求交。highspeed 变体继承基价行。空交集是 `[]`，不是错误。它 **不是** 带鉴权的 `GET /v1/models`：后者公布当前可路由的 Go 别名、已保存 Zen Free 目录与合格 Custom 声明 ID。两条路径都是本地读取：不含未公布的 Command Code GOAT 名称，也不在上游实时抓目录或挑账号。只返回当前可路由且协议有效启用的模型。Zen Free 目录刷新是 **供应商** 页上的显式动作；价格刷新后，这里的 Go 别名可能变化。每次返回应用页都会重新加载这份本地列表。模型选择和编辑过的代码片段按应用缓存在当前页面会话里，刷新即重置。**恢复默认** 重置当前应用的模型选择与片段草稿。

## 模型能力

下面这张限额表来自 `src/views/application-guides.ts`（2026-08-14），已核对过。输入列是 OCG 实际能带上的模态。透传 / 转换矩阵见 [协议转换](protocol-conversion.zh-CN.md#协议转换)。

| 模型 | 上下文 | 输出 | 输入 | 推理 | 工具 | 力度 |
| --- | ---: | ---: | --- | --- | :---: | --- |
| `grok-4.5` | 500K | 500K | 文本、图像 | 始终 | ✓ | low / medium / high（默认 high） |
| `gpt-5.6-luna` | 1.05M | 128K | 文本、图像 | ✓ | ✓ | low / medium / high / max（默认 medium） |
| `muse-spark-1.2` | 1M | 128K | 文本、图像 | ✓ | ✓ | low / medium / high（默认 high） |
| `muse-spark-1.2-contributor` | 1M | 128K | 文本、图像 | ✓ | ✓ | low / medium / high（默认 high） |
| `glm-5.3` | 1M | 128K | 文本 | ✓ | ✓ | low / high / max（默认 max） |
| `glm-5.2` | 1M | 128K | 文本 | ✓ | ✓ | high / max（默认 max） |
| `glm-5.1` | 198K | 32K | 文本 | ✓ | ✓ | — |
| `kimi-k3` | 1M | 128K | 文本、图像、视频 | 始终 | ✓ | max |
| `kimi-k2.7-code` | 256K | 256K | 文本、图像、视频 | 始终 | ✓ | — |
| `kimi-k2.6` | 256K | 64K | 文本、图像、视频 | ✓ | ✓ | — |
| `mimo-v2.5` | 1M | 128K | 文本、图像、音频、视频 | ✓ | ✓ | — |
| `mimo-v2.5-pro` | 1M | 128K | 文本 | ✓ | ✓ | — |
| `minimax-m3` | 1M | 128K | 文本、图像 | ✓ | ✓ | — |
| `minimax-m2.7` | 200K | 128K | 文本 | 始终 | ✓ | — |
| `minimax-m2.7-highspeed` | 200K | 128K | 文本 | 始终 | ✓ | — |
| `minimax-m2.5` | 200K | 64K | 文本 | 始终 | ✓ | — |
| `minimax-m2.5-highspeed` | 200K | 64K | 文本 | 始终 | ✓ | — |
| `qwen3.8-max` | 1M | 128K | 文本 | ✓ | ✓ | — |
| `qwen3.7-max` | 1M | 64K | 文本 | ✓ | ✓ | — |
| `qwen3.7-plus` | 1M | 64K | 文本、图像 | ✓ | ✓ | — |
| `qwen3.6-plus` | 1M | 64K | 文本、图像 | ✓ | ✓ | — |
| `deepseek-v4-pro` | 1M | 384K | 文本 | ✓ | ✓ | high / max（默认 high） |
| `deepseek-v4-flash` | 1M | 384K | 文本 | ✓ | ✓ | high / max（默认 high） |
| `hy3` | 256K | 64K | 文本 | ✓ | ✓ | low / high（默认 high） |

`muse-spark-1.2` 使用零数据保留（ZDR）：提示词和补全内容不会用于训练。 `muse-spark-1.2-contributor` 不使用 ZDR；提示词和补全内容可能用于训练。仅在你有权这样使用的数据上选择 Contributor。Muse 标准价格来自实时 Go 用量测量，因为公开 Go 价格表只列出 Contributor。

显示取整：198K = 202,752；200K = 204,800；256K = 262,144；1M = 1,000,000 或 1,048,576。`glm-5.3` 的限额与 token 单价已按 models.dev `opencode-go/glm-5.3` 独立行对齐（$1.40 / $4.40 / $0.26，与官方 Go 表一致；Usage 仍为 $15）。

Claude Desktop 是例外，它的模型映射是持久化的：复制配置前，选中的 `sonnet`、 `opus`、`haiku` 目标模型会通过受保护的 `GET/PUT /dashboard/api/v3/claude-desktop/models` 保存到 SQLite。留空的角色回退到第一个已配置模型，三个角色不能同时为空。它的恢复操作回到当前页面已加载或最后保存的映射。

---

[用户指南索引](../USER.zh-CN.md) · [English](applications.md) · [文档索引](../README.zh-CN.md)
