[English](README.md)

# OCG Manager

一个本地 Gateway：把你的各厂商 API Key 收进一个 SQLite 数据库，在一个端口（`http://127.0.0.1:9042`）上讲五种客户端协议——这样你机器上的每个 AI 工具就不必再假装自己管得住 Key 了。

每张账号卡绑定一个 Plan（provider/offering），Plan 需要时再存一份凭据。客户端用 OpenAI、Anthropic、Gemini 或 Claude Desktop 协议发送本地别名；Gateway 把请求转到该 Plan 的上游协议，再把响应转回来。当前可路由：OpenCode Go、Zen Free、Custom API。无遥测、无远端同步——Key 不出本机。

<p align="center">
  <a href="https://github.com/klarkxy/opencode-go-mgr">
    <img src="assets/star.webp" alt="去 GitHub 给本仓库点个 Star" width="420">
  </a>
</p>

## 主要特性

- **一个端口，五种线协议**：OpenAI Chat Completions、OpenAI Responses、Anthropic Messages、Gemini `generateContent` / `streamGenerateContent`，以及 Claude Desktop。客户端永远不需要知道上游想要哪一种。
- **拖动即调序**：账号卡片持久保存一个全局顺序；严格优先、粘性、轮询都在能力过滤后复用它。
- **额度条是警告，不是墙**：5 小时 / 本周 / 本月用量只是本地估算。满格不停流量；只有上游 `429` 才会让账号冷却。
- **16 个客户端教程**：Claude Code、Codex、Gemini CLI 等 16 个工具，配置片段直接复制。
- **桌面端、CLI、Docker**：Tauri v2 托盘应用、`ocg-manager-cli`、`ghcr.io/klarkxy/opencode-go-mgr`。已安装的桌面版可在设置页完成签名自更新。

## 下载

从 [GitHub 最新 Release](https://github.com/klarkxy/opencode-go-mgr/releases/latest) 下载 GUI 安装包或 CLI 压缩包，并用同一 Release 的 `SHA256SUMS` 校验（PowerShell 用 `Get-FileHash <文件> -Algorithm SHA256`,macOS 用 `shasum -a 256`,Linux 用 `sha256sum`）：

| 平台 | GUI | CLI |
| --- | --- | --- |
| Windows 10/11 x64 | `ocg-manager_<version>_windows-x64-setup.exe`（NSIS） | `ocg-manager-cli_<version>_windows-x64.zip` |
| macOS 11+ Intel 与 Apple Silicon | `ocg-manager_<version>_macos-universal.dmg` | `ocg-manager-cli_<version>_macos-universal.tar.gz` |
| Linux x64 | `ocg-manager_<version>_linux-x64.AppImage` 和 `.deb` | `ocg-manager-cli_<version>_linux-x64.tar.gz` |

CLI 的 `dist/` 必须与可执行文件同级，否则 `serve` 没有面板可服务。平台注意事项（SmartScreen、Gatekeeper、无 ARM64 / RPM / Snap / 应用商店）见[用户指南](docs/user/install.zh-CN.md)。

## 快速开始

```text
Gateway: http://127.0.0.1:9042/v1
鉴权:    Authorization: Bearer <key>
```

1. 安装并启动。Gateway 就绪后管理面板会在系统浏览器中打开；托盘图标随时唤回。
2. 在 **账号** 视图导入 OpenCode Go 账号 Key、选择无需凭据的 Zen Free，或添加 Custom API 目的地（先验证再启用）。复制 **Key**——这是客户端唯一需要知道的秘密。
3. 把客户端指向 `http://127.0.0.1:9042/v1`。**应用** 视图有各客户端的配置教程。

```bash
curl http://127.0.0.1:9042/v1/chat/completions \
  -H "Authorization: Bearer ocg-xxxxxxxx-xxxxxxxx" \
  -H "Content-Type: application/json" \
  -d '{"model":"glm-5.2","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

安装细节、五种协议的最小检查、备份与升级见[用户指南](docs/USER.zh-CN.md)。

## Docker

把 [`compose.example.yaml`](compose.example.yaml) 保存为 `compose.yaml`，然后：

```bash
docker compose pull
docker compose up -d --no-build
```

镜像：`ghcr.io/klarkxy/opencode-go-mgr`（`linux/amd64, linux/arm64`，匿名可拉）。打开 `http://127.0.0.1:9042/dashboard/`——裸的 `/` 不是面板。浏览器 Sidecar、备份、HTTPS、镜像钉、源码构建见 [用户指南 · Docker](docs/user/docker.zh-CN.md)。

## 模型

每个 OpenCode Go 模型都有硬编码的 **推荐上游协议** 和实测的 **已验证可用协议集合**：客户端协议落在集合内就透传，否则转换。请求路径绝不试探协议——那可能把同一请求计两次费。

| 推荐上游协议 | 模型 |
| --- | --- |
| OpenAI Chat Completions | `glm-5.3`、`glm-5.2`、`glm-5.1`、`glm-5`、`kimi-k3`、`kimi-k2.7-code`、`kimi-k2.6`、`kimi-k2.5`、`deepseek-v4-pro`、`deepseek-v4-flash`、`mimo-v2.5`、`mimo-v2.5-pro`、`hy3`、`ox-alpha-free` |
| OpenAI Responses | `grok-4.5`、`gpt-5.6-luna`、`muse-spark-1.2`、`muse-spark-1.2-contributor` |
| Anthropic Messages | `minimax-m3`、`minimax-m2.7`、`minimax-m2.7-highspeed`、`minimax-m2.5`、`minimax-m2.5-highspeed`、`qwen3.8-max`、`qwen3.7-max`、`qwen3.7-plus`、`qwen3.6-plus`、`qwen3.5-plus` |

是的，`ox-alpha-free` 名字里带 `free` 却照样走 `/zen/go`——起名是件难事。Zen Free 没有固定列表：由管理员显式刷新官方目录，最后一次成功的快照生效。Gemini 只是客户端格式，不会有任何请求发往 Google。透传矩阵、能力表、转换边界见 [模型能力](docs/user/applications.zh-CN.md)与 [协议转换](docs/user/protocol-conversion.zh-CN.md)。

## 文档

| 读者 | English | 简体中文 |
| --- | --- | --- |
| 终端用户 | [User guide](docs/USER.md) | [用户指南](docs/USER.zh-CN.md) |
| 维护者 | [Maintainer guide](docs/MAINTAINER.md) | [维护者指南](docs/MAINTAINER.zh-CN.md) |
| 使用边界 | [Anti-abuse statement](docs/OPENCODE_GO_ANTI_ABUSE.md) | [防滥用声明](docs/OPENCODE_GO_ANTI_ABUSE.zh-CN.md) |
| 文档索引 | [docs/](docs/README.md) | [文档索引](docs/README.zh-CN.md) |

另见：[Contributors](docs/CONTRIBUTORS.md)、[DESIGN.md](DESIGN.md)、[AGENTS.md](AGENTS.md)。

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

先退出 release 托盘程序——单实例锁和 `9042` 端口都不接受合租。检查、构建与发布流水线见[维护者指南](docs/MAINTAINER.zh-CN.md)。

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
