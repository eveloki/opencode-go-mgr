[简体中文](README.zh-CN.md)

# OCG Manager

A local gateway that keeps your provider API keys in one SQLite database and
speaks five client protocols on one port (`http://127.0.0.1:9042`) — so every
AI tool on your machine can stop pretending it has its own key manager.

Each account card binds one Plan (provider/offering) and, when the Plan needs
one, one credential. Clients speak OpenAI, Anthropic, Gemini, or Claude
Desktop and send local aliases; the gateway converts each request to the
Plan's upstream protocol and the answer back. Live routing: OpenCode Go,
Zen Free, Command Code GOAT, MiniMax CN Token Plan, Kimi Code CN, and Custom
API. No telemetry, no remote sync — your keys never leave the machine.

<p align="center">
  <a href="https://github.com/klarkxy/opencode-go-mgr">
    <img src="assets/star.webp" alt="Star this repository on GitHub" width="420">
  </a>
</p>

## Highlights

- **One port, five wire formats** — OpenAI Chat Completions, OpenAI Responses,
  Anthropic Messages, Gemini `generateContent` / `streamGenerateContent`, and
  Claude Desktop. Your clients never learn which one the upstream wanted.
- **Drag to reroute** — account cards persist one global order; strict
  priority, sticky, and round-robin reuse it after capability filtering.
- **Quota bars are warnings, not walls** — 5-hour / weekly / monthly usage is
  a local estimate. A full bar stops nothing; only an upstream `429` cools an
  account down.
- **16 client guides** — copy-ready snippets for Claude Code, Codex, Gemini
  CLI, and 13 other tools.
- **Desktop, CLI, Docker** — a Tauri v2 tray app, `ocg-manager-cli`, and
  `ghcr.io/klarkxy/opencode-go-mgr`. Installed desktops update themselves,
  signed, from Settings.

## Download

Grab the GUI installer or CLI archive from the
[latest GitHub Release](https://github.com/klarkxy/opencode-go-mgr/releases/latest)
and check it against that release's `SHA256SUMS` (`Get-FileHash <file>
-Algorithm SHA256` on PowerShell, `shasum -a 256` on macOS, `sha256sum` on
Linux):

| Platform | GUI | CLI |
| --- | --- | --- |
| Windows 10/11 x64 | `ocg-manager_<version>_windows-x64-setup.exe` (NSIS) | `ocg-manager-cli_<version>_windows-x64.zip` |
| macOS 11+ Intel and Apple Silicon | `ocg-manager_<version>_macos-universal.dmg` | `ocg-manager-cli_<version>_macos-universal.tar.gz` |
| Linux x64 | `ocg-manager_<version>_linux-x64.AppImage` and `.deb` | `ocg-manager-cli_<version>_linux-x64.tar.gz` |

Keep `dist/` beside the CLI executable, or `serve` has no dashboard to serve.
Platform caveats (SmartScreen, Gatekeeper, no ARM64 / RPM / Snap / stores):
[User guide](docs/user/install.md).

## Quick Start

```text
Gateway: http://127.0.0.1:9042/v1
Auth:    Authorization: Bearer <key>
```

1. Install and launch. The dashboard opens in your system browser once the
   gateway is ready; the tray icon brings it back.
2. In **Accounts**, import an OpenCode Go account key, pick credential-free
   Zen Free, or add a Custom API destination (declare protocols and models,
   then enable — verification is optional). Copy the
   **Key** — the only secret your client ever sees.
3. Point your client at `http://127.0.0.1:9042/v1`. **Applications** has
   per-client setup guides.

```bash
curl http://127.0.0.1:9042/v1/chat/completions \
  -H "Authorization: Bearer ocg-xxxxxxxx-xxxxxxxx" \
  -H "Content-Type: application/json" \
  -d '{"model":"glm-5.2","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

Install details, per-protocol first-client checks, backup, and upgrades:
[User guide](docs/USER.md).

## Docker

Save [`compose.example.yaml`](compose.example.yaml) as `compose.yaml` and run:

```bash
docker compose pull
docker compose up -d --no-build
```

Image: `ghcr.io/klarkxy/opencode-go-mgr` (`linux/amd64, linux/arm64`,
anonymous pull). Open `http://127.0.0.1:9042/dashboard/` — bare `/` is not
the dashboard. Browser sidecar, backup, HTTPS, image pins, source builds:
[User guide — Docker](docs/user/docker.md).

## Models

Each OpenCode Go model has a hardcoded **preferred** upstream protocol and a
probed **supported** set: matching client protocols pass through, the rest
get converted. The gateway never probes at request time — that could
double-bill.

| Preferred upstream | Models |
| --- | --- |
| OpenAI Chat Completions | `glm-5.3`, `glm-5.2`, `glm-5.1`, `glm-5`, `kimi-k3`, `kimi-k2.7-code`, `kimi-k2.6`, `kimi-k2.5`, `deepseek-v4-pro`, `deepseek-v4-flash`, `mimo-v2.5`, `mimo-v2.5-pro`, `hy3`, `ox-alpha-free` |
| OpenAI Responses | `grok-4.5`, `gpt-5.6-luna`, `muse-spark-1.2`, `muse-spark-1.2-contributor` |
| Anthropic Messages | `minimax-m3`, `minimax-m2.7`, `minimax-m2.7-highspeed`, `minimax-m2.5`, `minimax-m2.5-highspeed`, `qwen3.8-max`, `qwen3.7-max`, `qwen3.7-plus`, `qwen3.6-plus`, `qwen3.5-plus` |

Yes, `ox-alpha-free` says `free` and still rides `/zen/go` — naming is hard.
Zen Free has no fixed list: an administrator refreshes the official catalog
and the last good snapshot wins. Gemini is a client format only; nothing is
sent to Google. Passthrough matrix, capabilities, and conversion limits:
[model capabilities](docs/user/applications.md) and
[protocol conversion](docs/user/protocol-conversion.md).

## Documentation

| Audience | English | 简体中文 |
| --- | --- | --- |
| End users | [User guide](docs/USER.md) | [用户指南](docs/USER.zh-CN.md) |
| Maintainers | [Maintainer guide](docs/MAINTAINER.md) | [维护者指南](docs/MAINTAINER.zh-CN.md) |
| Policy | [Anti-abuse statement](docs/OPENCODE_GO_ANTI_ABUSE.md) | [防滥用声明](docs/OPENCODE_GO_ANTI_ABUSE.zh-CN.md) |
| Index | [docs/](docs/README.md) | [文档索引](docs/README.zh-CN.md) |

Also: [Contributors](docs/CONTRIBUTORS.md), [DESIGN.md](DESIGN.md),
[AGENTS.md](AGENTS.md).

## Community

Join the OCG Manager QQ group: **1104321231**.

<p align="center">
  <img src="assets/qq-group.png" alt="OCG Manager QQ group QR code" width="360" />
</p>

## Development

```bash
pnpm install
pnpm run dev
```

Quit the release tray app first — the single-instance lock and port `9042`
don't share. Checks, builds, and the release pipeline:
[Maintainer guide](docs/MAINTAINER.md).

## License

See [LICENSE](LICENSE).

## Star History

<a href="https://www.star-history.com/?type=date&repos=klarkxy%2Fopencode-go-mgr">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=klarkxy/opencode-go-mgr&type=date&theme=dark&legend=top-left&sealed_token=oIYrocSP1u8BIlRFlVg34QKt9W7GAzchQqPbmV-cwy6F84-IJx1RTsYIEG0UYpaFcFPiCY24bdJgYhkONvQgjsIQzgRLf_YXiP7W9BzlHU9rMGGb68O2Tg" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=klarkxy/opencode-go-mgr&type=date&legend=top-left&sealed_token=oIYrocSP1u8BIlRFlVg34QKt9W7GAzchQqPbmV-cwy6F84-IJx1RTsYIEG0UYpaFcFPiCY24bdJgYhkONvQgjsIQzgRLf_YXiP7W9BzlHU9rMGGb68O2Tg" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=klarkxy/opencode-go-mgr&type=date&legend=top-left&sealed_token=oIYrocSP1u8BIlRFlVg34QKt9W7GAzchQqPbmV-cwy6F84-IJx1RTsYIEG0UYpaFcFPiCY24bdJgYhkONvQgjsIQzgRLf_YXiP7W9BzlHU9rMGGb68O2Tg" />
 </picture>
</a>
