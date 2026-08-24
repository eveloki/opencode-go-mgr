[English](cli.md)

# CLI

下载对应平台的压缩包并解压成目录，目录里有可执行文件、`dist/` 与 `LICENSE`。 `dist/` 必须与可执行文件同级，`serve` 才能提供管理面板。Windows 上可执行文件是 `ocg-manager-cli.exe`；Linux 解压后可能需要 `chmod +x ocg-manager-cli`。

CLI 数据目录默认在 `~/.ocg-mgr-cli`（所有平台一致），可用 `--data-dir <path>` 覆盖。混淆密钥默认保存在 `<data-dir>/.encryption-key`，可用名为 `--encryption-key <key>` 的参数或 `OCG_MANAGER_ENCRYPTION_KEY` 环境变量覆盖。

CLI 命令面只有 `serve`、`key` 与 `status`。`key` 管理 OpenCode Go 账号凭据，不是面板接入 Key，也不能创建 Custom 或操作 Zen Free 卡片。接入 Key、Custom 目的地、协议开关与目录仍在面板里完成。CLI 账号写入会 bump 该进程的 settings revision，命令行不接受 `expectedRevision`。

```text
ocg-manager-cli
├── serve         Start the gateway server
│   --host        Address to listen on (default 127.0.0.1)
│   -p, --port    Gateway port (sets and saves config)
│   --dashboard-dir  Directory containing the built web dashboard
├── key list      List OpenCode Go API-key accounts (excludes Zen Free)
├── key add <name> <key>
│   --username    OpenCode-Go login account
│   --password    OpenCode-Go login password
├── key remove <id>      Remove an account
├── key enable <id>      Enable an account
├── key disable <id>     Disable an account
├── key ping [id]
│   --model       Model to send (default mimo-v2.5)
│   --message     User message (default "ping")
│   --max-tokens  max_tokens for the ping (default 3)
└── status        Show data dir, gateway port/key, upstream, account totals
```

最快搭出一个无头 Gateway：

```bash
./ocg-manager-cli key add main sk-...
./ocg-manager-cli key list
./ocg-manager-cli serve --port 9042
```

`serve --port <port>` 会把新端口写入 SQLite；之后不带 `--port` 的 `serve` 会继续使用该值。

`key ping` 会读取混淆后的 Key、发送一条极小的 chat completion、打印真实的上游状态码与一段响应体摘要——绕过面板直接拿到每个 Key 真实的 `401`/`403`/`429`/`200`。

---

[用户指南索引](../USER.zh-CN.md) · [English](cli.md) · [文档索引](../README.zh-CN.md)
