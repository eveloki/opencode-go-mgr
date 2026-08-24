[简体中文](README.zh-CN.md)

# OCG Manager

OCG Manager is a local operations console for provider API-key accounts. Each
account card binds one Plan (provider/offering) and, when that Plan requires
one, one credential in SQLite. It
serves a multi-protocol gateway — plus the management dashboard — at
`http://127.0.0.1:9042`. Clients speak OpenAI, Anthropic, Gemini, or Claude
Desktop and send local aliases; the gateway converts each request to the
selected Plan's supported upstream protocol and converts the response back.
Live routing is OpenCode Go, Zen Free, and Custom API.

<p align="center">
  <a href="https://github.com/klarkxy/opencode-go-mgr">
    <img src="assets/star.webp" alt="Star this repository on GitHub" width="420">
  </a>
</p>

## Highlights

- **One port, five wire formats** — OpenAI Chat Completions, OpenAI Responses,
  Anthropic Messages, Gemini `generateContent` / `streamGenerateContent`, and
  Claude Desktop.
- **Local multi-account routing** — drag account cards to persist one global
  order; strict priority, sticky, and round-robin reuse it after capability
  filtering.
- **Quota bars are warnings** — 5-hour / weekly / monthly usage is a local
  estimate. A full bar does not stop traffic; only an upstream `429` cools
  an account down.
- **16 client guides** — copy-ready snippets for Claude Code, Codex, Gemini
  CLI, and 13 other tools.
- **Desktop, CLI, and Docker** — a Tauri v2 tray app, `ocg-manager-cli`, and
  `ghcr.io/klarkxy/opencode-go-mgr`. Installed desktop builds can install
  signed updates from Settings.
- **No remote sync, no telemetry** — every node owns its own data. Managed
  onboarding is Beta; do not rely on it in production.

## Download

Download the GUI installer or CLI archive from the
[latest GitHub Release](https://github.com/klarkxy/opencode-go-mgr/releases/latest)
and verify it against that release's `SHA256SUMS` before installing:
`Get-FileHash <file> -Algorithm SHA256` on PowerShell, `shasum -a 256 <file>`
on macOS, or `sha256sum <file>` on Linux.

| Platform | GUI | CLI |
| --- | --- | --- |
| Windows 10/11 x64 | `ocg-manager_<version>_windows-x64-setup.exe` (NSIS) | `ocg-manager-cli_<version>_windows-x64.zip` |
| macOS 11+ Intel and Apple Silicon | `ocg-manager_<version>_macos-universal.dmg` | `ocg-manager-cli_<version>_macos-universal.tar.gz` |
| Linux x64 | `ocg-manager_<version>_linux-x64.AppImage` and `.deb` | `ocg-manager-cli_<version>_linux-x64.tar.gz` |

Keep `dist/` beside the CLI executable so `serve` can serve the dashboard.
Platform caveats (SmartScreen, Gatekeeper, unsigned Windows, no ARM64 / RPM /
Snap / stores) are in the [User guide](docs/USER.md#install-and-first-run) and
[Maintainer guide](docs/MAINTAINER.md).

## Quick Start

```text
Gateway: http://127.0.0.1:9042/v1
Auth:    Authorization: Bearer <key>
```

The dashboard **Key** is the only secret a client needs. For live traffic,
import an OpenCode Go account key; Zen Free sends no upstream credential.
Custom API is a trusted-administrator destination: save, verify, then enable.

1. Install and launch OCG Manager. The dashboard opens in your system browser
   once the gateway is ready; use the tray icon to reopen it.
2. In **Accounts**, import an OpenCode Go account key, choose credential-free
   Zen Free, add a Custom API destination (verify, then enable), or use
   managed onboarding (Beta). Copy the Key.
3. Point your client at `http://127.0.0.1:9042/v1`. **Applications** has
   per-client configuration guides.

```bash
curl http://127.0.0.1:9042/v1/chat/completions \
  -H "Authorization: Bearer ocg-xxxxxxxx-xxxxxxxx" \
  -H "Content-Type: application/json" \
  -d '{"model":"glm-5.2","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

Install, first-client checks for all five protocols, backup, and upgrades:
[User guide](docs/USER.md).

## Docker

Public image: `ghcr.io/klarkxy/opencode-go-mgr` (`linux/amd64, linux/arm64`,
anonymous pull). Save [`compose.example.yaml`](compose.example.yaml) (also
attached to each Release) as `compose.yaml` and run:

```bash
docker compose pull
docker compose up -d --no-build
```

Open `http://127.0.0.1:9042/dashboard/` — the server root `/` is not the
dashboard. Credentials, the optional browser sidecar, backup, HTTPS, image
pins, and source builds: [User guide — Docker](docs/USER.md#docker).

## Models

OpenCode Go models have a hardcoded **preferred** upstream protocol and a
probed **supported** set. Matching client protocols passthrough; others
convert. The gateway never probes a protocol at request time — that could
double-bill.

| Preferred upstream | Models |
| --- | --- |
| OpenAI Chat Completions | `glm-5.3`, `glm-5.2`, `glm-5.1`, `glm-5`, `kimi-k3`, `kimi-k2.7-code`, `kimi-k2.6`, `kimi-k2.5`, `deepseek-v4-pro`, `deepseek-v4-flash`, `mimo-v2.5`, `mimo-v2.5-pro`, `hy3`, `ox-alpha-free` |
| OpenAI Responses | `grok-4.5`, `gpt-5.6-luna`, `muse-spark-1.2`, `muse-spark-1.2-contributor` |
| Anthropic Messages | `minimax-m3`, `minimax-m2.7`, `minimax-m2.7-highspeed`, `minimax-m2.5`, `minimax-m2.5-highspeed`, `qwen3.8-max`, `qwen3.7-max`, `qwen3.7-plus`, `qwen3.6-plus`, `qwen3.5-plus` |

`ox-alpha-free` (Ox Alpha Free) is a Go Chat model; the name contains `free`
but it stays on `/zen/go`. Zen Free is not a fixed model list: an administrator
explicitly refreshes its official catalog, and the manager persists the last
successful normalized `-free` snapshot. A failed or empty refresh retains the
previous snapshot; published aliases and protocol entries come from that saved
catalog.

Gemini is a client format only (requests never go to Google). Claude Desktop
aliases are rewritten to the mapping saved in **Applications**. Clients should
send published aliases or eligible Custom IDs; authenticated `GET /v1/models`
lists currently routeable published aliases (OpenCode Go and Zen Free) plus
eligible Custom IDs from enabled, verified, ready accounts with a key (no
upstream discovery). Unknown names return `400` unless they match that list.
The Applications picker stays Go aliases ∩ the active pricing snapshot.

Passthrough matrix, context / input / reasoning / tools, conversion limits,
and true vs false circuit breakers:
[User guide — model capabilities](docs/USER.md#model-capabilities) and
[protocol conversion](docs/USER.md#protocol-conversion).

## Documentation

| Audience | English | 简体中文 |
| --- | --- | --- |
| End users | [User guide](docs/USER.md) | [用户指南](docs/USER.zh-CN.md) |
| Maintainers | [Maintainer guide](docs/MAINTAINER.md) | [维护者指南](docs/MAINTAINER.zh-CN.md) |
| Schema v27 runbook | [V3 schema recovery](docs/MAINTAINER-v3-migration.md) | [V3 库恢复](docs/MAINTAINER-v3-migration.zh-CN.md) |
| Policy | [Anti-abuse statement](docs/OPENCODE_GO_ANTI_ABUSE.md) | [防滥用声明](docs/OPENCODE_GO_ANTI_ABUSE.zh-CN.md) |
| Index | [docs/](docs/README.md) | bilingual |

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

Exit any running release tray app first so the single-instance lock and port
`9042` are free. Tauri starts Vite and opens
`http://127.0.0.1:30001/dashboard/` once the gateway is ready. The dashboard
SPA talks HTTP `/dashboard/api/v3`; authenticated unversioned
`/dashboard/api` REST is retired. Checks, builds, and the release pipeline:
[Maintainer guide](docs/MAINTAINER.md). Schema v27 runbook:
[V3 schema recovery](docs/MAINTAINER-v3-migration.md).

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
