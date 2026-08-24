[English](README.md)

# OCG Manager

OCG Manager 是一个本地多 Plan 运维控制台。每张账号卡绑定一个 Plan（provider/offering），并在该 Plan 需要时于 SQLite 中保存一份凭据；它在 `http://127.0.0.1:9042` 同时提供多协议 Gateway 与管理面板。客户端使用 OpenAI、Anthropic、Gemini 或 Claude Desktop 协议并发送本地别名；Gateway 把请求转换到所选 Plan 支持的上游协议，再把响应转回客户端。当前可路由的是 OpenCode Go、Zen Free 与 Custom API。

<p align="center">
  <a href="https://github.com/klarkxy/opencode-go-mgr">
    <img src="assets/star.webp" alt="去 GitHub 给本仓库点个 Star" width="420">
  </a>
</p>

## 主要特性

- **一个端口，五种线协议格式**：OpenAI Chat Completions、OpenAI Responses、 Anthropic Messages、Gemini `generateContent` / `streamGenerateContent`，以及 Claude Desktop。
- **本地多账号轮询**：拖动账号卡片即可持久调整优先级；Gateway 自动跳过已禁用、冷却中或本次请求已失败的账号。
- **额度条只是警告**：5 小时 / 本周 / 本月用量是本地估算。满格不会停流量；只有上游 `429` 才会让账号进入冷却。
- **16 个应用配置教程**：Claude Code、Codex、Gemini CLI 等 16 个客户端可直接复制配置片段。
- **桌面端、CLI 与 Docker**：Tauri v2 托盘应用、`ocg-manager-cli`，以及 `ghcr.io/klarkxy/opencode-go-mgr`。已安装的桌面版可在设置页安装签名更新。
- **无远端同步、无遥测**：每个节点独立管理自己的数据。托管注册仍是 Beta，不建议用于生产环境。

## 下载

从 [GitHub 最新 Release](https://github.com/klarkxy/opencode-go-mgr/releases/latest) 下载对应平台的 GUI 安装包或 CLI 压缩包，安装前用同一 Release 的 `SHA256SUMS` 校验：PowerShell 使用 `Get-FileHash <文件> -Algorithm SHA256`，macOS 使用 `shasum -a 256 <文件>`，Linux 使用 `sha256sum <文件>`。

| 平台 | GUI | CLI |
| --- | --- | --- |
| Windows 10/11 x64 | `ocg-manager_<version>_windows-x64-setup.exe`（NSIS） | `ocg-manager-cli_<version>_windows-x64.zip` |
| macOS 11+ Intel 与 Apple Silicon | `ocg-manager_<version>_macos-universal.dmg` | `ocg-manager-cli_<version>_macos-universal.tar.gz` |
| Linux x64 | `ocg-manager_<version>_linux-x64.AppImage` 和 `.deb` | `ocg-manager-cli_<version>_linux-x64.tar.gz` |

CLI 的 `dist/` 必须与可执行文件同级，`serve` 才能提供管理面板。平台注意（SmartScreen、Gatekeeper、Windows 未签名、无 ARM64 / RPM / Snap / 应用商店）见[用户指南](docs/user/install.zh-CN.md)和 [维护者指南](docs/MAINTAINER.zh-CN.md)。

## 快速开始

```text
Gateway: http://127.0.0.1:9042/v1
鉴权:    Authorization: Bearer <key>
```

面板里的 **Key** 是客户端唯一需要配置的密钥。实时流量可导入 OpenCode Go 账号 Key；Zen Free 不发送上游凭据。Custom API 是受信管理员目的地：先保存、再验证、然后启用。

1. 安装并启动 OCG Manager。Gateway 就绪后管理面板会在系统浏览器中打开；之后可通过托盘图标重新打开。
2. 在 **账号** 视图导入 OpenCode Go 账号 Key、选择无需凭据的 Zen Free、添加 Custom API 目的地（先验证再启用），或用托管向导注册（Beta）。复制 Key。
3. 把客户端指向 `http://127.0.0.1:9042/v1`。**应用** 视图提供各客户端配置教程。

```bash
curl http://127.0.0.1:9042/v1/chat/completions \
  -H "Authorization: Bearer ocg-xxxxxxxx-xxxxxxxx" \
  -H "Content-Type: application/json" \
  -d '{"model":"glm-5.2","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

安装、五种协议的最小检查、备份与升级见[用户指南](docs/USER.zh-CN.md)。

## Docker

公开镜像：`ghcr.io/klarkxy/opencode-go-mgr`（`linux/amd64, linux/arm64`，匿名即可拉取）。把 [`compose.example.yaml`](compose.example.yaml)（每个 Release 也会附带）保存为 `compose.yaml` 后运行：

```bash
docker compose pull
docker compose up -d --no-build
```

打开 `http://127.0.0.1:9042/dashboard/`；服务根路径 `/` 不是管理面板。管理员、可选浏览器 Sidecar、备份、HTTPS、镜像钉和源码构建见 [用户指南的 Docker 章节](docs/user/docker.zh-CN.md)。

## 模型

OpenCode Go 模型有硬编码的 **推荐上游协议** 与实测 **已验证可用协议集合**。客户端协议落在集合内时透传，否则转换到推荐协议。请求路径不会试探协议——否则可能把同一请求重复计费。

| 推荐上游协议 | 模型 |
| --- | --- |
| OpenAI Chat Completions | `glm-5.3`、`glm-5.2`、`glm-5.1`、`glm-5`、`kimi-k3`、`kimi-k2.7-code`、`kimi-k2.6`、`kimi-k2.5`、`deepseek-v4-pro`、`deepseek-v4-flash`、`mimo-v2.5`、`mimo-v2.5-pro`、`hy3`、`ox-alpha-free` |
| OpenAI Responses | `grok-4.5`、`gpt-5.6-luna`、`muse-spark-1.2`、`muse-spark-1.2-contributor` |
| Anthropic Messages | `minimax-m3`、`minimax-m2.7`、`minimax-m2.7-highspeed`、`minimax-m2.5`、`minimax-m2.5-highspeed`、`qwen3.8-max`、`qwen3.7-max`、`qwen3.7-plus`、`qwen3.6-plus`、`qwen3.5-plus` |

`ox-alpha-free`（Ox Alpha Free）是 Go 的 Chat 模型，名字里带 `free` 但仍走 `/zen/go`。Zen Free 不是固定模型列表：管理员显式刷新官方目录后，管理器会持久化最后一次成功的规范化 `-free` 快照。刷新失败或结果为空时保留旧快照；已公布的别名与协议条目均来自这份已保存目录。

Gemini 只是客户端格式（请求不会发往 Google）。Claude Desktop 别名会改写为 **应用** 视图里保存的映射。客户端应发送已公布别名或合格 Custom ID；带鉴权的 `GET /v1/models` 列出当前可路由的已公布别名（OpenCode Go 与 Zen Free），并并入已启用、已验证、 ready 且有 Key 的 Custom ID（不发现上游）。未知名称若不在该列表中，所有受支持的客户端格式都返回 `400`。应用选择器仍是 Go 别名 ∩ 当前价格快照。

透传矩阵、上下文 / 输入 / 推理 / 工具、转换边界，以及真/假熔断见 [用户指南 · 模型能力](docs/user/applications.zh-CN.md)与 [协议转换](docs/user/protocol-conversion.zh-CN.md)。

## 文档

| 读者 | English | 简体中文 |
| --- | --- | --- |
| 终端用户 | [User guide](docs/USER.md) | [用户指南](docs/USER.zh-CN.md) |
| 维护者 | [Maintainer guide](docs/MAINTAINER.md) | [维护者指南](docs/MAINTAINER.zh-CN.md) |
| 使用边界 | [Anti-abuse statement](docs/OPENCODE_GO_ANTI_ABUSE.md) | [防滥用声明](docs/OPENCODE_GO_ANTI_ABUSE.zh-CN.md) |
| 文档索引 | [docs/](docs/README.md) | [文档索引](docs/README.zh-CN.md) |

另见：[Contributors](docs/CONTRIBUTORS.md)、[DESIGN.md](DESIGN.md)、 [AGENTS.md](AGENTS.md)。

## 交流群

加入 OCG Manager QQ 群：**1104321231**。

<p align="center">
  <img src="assets/qq-group.png" alt="OCG Manager QQ 群二维码" width="360" />
</p>

## 开发模式

```bash
pnpm install
pnpm run dev
```

开发前先退出 release 托盘程序，释放单实例锁和 `9042` 端口。Tauri 会启动 Vite，并在 Gateway 就绪后打开 `http://127.0.0.1:30001/dashboard/`。面板 SPA 走 HTTP `/dashboard/api/v3`；已鉴权的未版本化 `/dashboard/api` REST 已退役。检查、构建与发布流水线见[维护者指南](docs/MAINTAINER.zh-CN.md)。

## 许可证

见 [LICENSE](LICENSE)。

## Star 历史

<a href="https://www.star-history.com/?type=date&repos=klarkxy%2Fopencode-go-mgr">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=klarkxy/opencode-go-mgr&type=date&theme=dark&legend=top-left&sealed_token=oIYrocSP1u8BIlRFlVg34QKt9W7GAzchQqPbmV-cwy6F84-IJx1RTsYIEG0UYpaFcFPiCY24bdJgYhkONvQgjsIQzgRLf_YXiP7W9BzlHU9rMGGb68O2Tg" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=klarkxy/opencode-go-mgr&type=date&legend=top-left&sealed_token=oIYrocSP1u8BIlRFlVg34QKt9W7GAzchQqPbmV-cwy6F84-IJx1RTsYIEG0UYpaFcFPiCY24bdJgYhkONvQgjsIQzgRLf_YXiP7W9BzlHU9rMGGb68O2Tg" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=klarkxy/opencode-go-mgr&type=date&legend=top-left&sealed_token=oIYrocSP1u8BIlRFlVg34QKt9W7GAzchQqPbmV-cwy6F84-IJx1RTsYIEG0UYpaFcFPiCY24bdJgYhkONvQgjsIQzgRLf_YXiP7W9BzlHU9rMGGb68O2Tg" />
 </picture>
</a>
