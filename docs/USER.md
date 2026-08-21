[简体中文](USER.zh-CN.md)

# User Guide

This guide is for people running OCG Manager as a desktop app, a headless
gateway, or a Docker service. It walks through installation, daily use, and
troubleshooting in the order you will meet them, and explains how the gateway,
the true / false circuit breakers, and protocol conversion actually work.

## Table Of Contents

- [What It Does](#what-it-does)
- [Install And First Run](#install-and-first-run)
- [Connect Your First Client](#connect-your-first-client)
- [Upgrade, Backup, Restore, And Uninstall](#upgrade-backup-restore-and-uninstall)
- [The Dashboard](#the-dashboard)
  - [Connection Center](#connection-center)
  - [Access Keys](#access-keys)
  - [Application Guides](#application-guides)
  - [Model capabilities](#model-capabilities)
  - [Accounts](#accounts)
  - [Pricing](#pricing)
  - [Logs](#logs)
  - [Settings](#settings)
- [Gateway Behavior](#gateway-behavior)
  - [Endpoints](#endpoints)
  - [Authentication](#authentication)
  - [Aliases](#aliases)
  - [Protocol Conversion](#protocol-conversion)
  - [Account Selection And Failover](#account-selection-and-failover)
  - [Cost Accounting](#cost-accounting)
  - [True And False Circuit Breakers](#true-and-false-circuit-breakers)
  - [Free model policy](#free-model-policy)
- [CLI](#cli)
- [Docker](#docker)
  - [Optional Remote Browser](#optional-remote-browser)
- [Data And Security](#data-and-security)
- [Limits](#limits)
- [Troubleshooting](#troubleshooting)

## What It Does

OCG Manager keeps provider API keys in a local SQLite database — officially
distributable OpenCode Go keys, plus trusted Custom API destinations — and
exposes a loopback gateway at `http://127.0.0.1:9042/v1`. Each
account card is one **Plan** (provider + offering). Clients send **aliases**
from the local registry or eligible Custom model IDs; live routing is
OpenCode Go, OpenCode Zen Free, and Custom API.
The same gateway also serves the Vue 3 dashboard at `/dashboard/` and its JSON
API at `/dashboard/api`. Every node is independent: there is no remote sync,
no Admin API, and no telemetry.

The gateway does four jobs:

1. Authenticate the client with the **Key** issued by the dashboard.
2. Resolve the requested model against the local Alias registry (and eligible
   Custom declared IDs), then pick a usable account card after capability
   filtering.
3. Convert the request to the selected Plan's supported upstream protocol,
   and the response back to the client protocol.
4. Log the request (`requested_model`, `resolved_alias`, `upstream_model`),
   write usage and any cooldown to SQLite, and surface everything in the
   dashboard.

## Install And First Run

### Windows 10/11 x64

1. Run the NSIS setup `ocg-manager_<version>_windows-x64-setup.exe`. It
   installs for the current user without administrator rights.
2. Launch **OCG Manager** from the Start menu. The dashboard opens in your
   system browser; use the tray icon to open it again later.
3. Current Windows builds are unsigned, so SmartScreen may warn. Click
   **More info → Run anyway** to continue.
4. Add an OpenCode-Go account in the **Accounts** view, copy the Key,
   and point your client at `http://127.0.0.1:9042/v1`.
5. The uninstaller asks whether to delete `%USERPROFILE%\.ocg-mgr`; silent
   upgrades and uninstalls preserve it.

### macOS 11+ Intel / Apple Silicon

1. Open the Universal DMG and drag **OCG Manager** to **Applications**.
2. The app is ad-hoc signed, so the first launch may be blocked. Open
   **Privacy & Security** and click **Open Anyway**.
3. Launch the app. The dashboard opens in your system browser; use the tray
   icon to reopen it. Add an account, copy the Key, and configure
   your client.

### Linux x64

1. Verify the download against `SHA256SUMS` first.
2. Install the `.deb` with your package manager, or mark the AppImage
   executable with `chmod +x ocg-manager_<version>_linux-x64.AppImage`.
3. Launch the executable. The dashboard opens in your system browser; use the
   tray icon to reopen it.
4. Data lives in `~/.ocg-mgr/`.

The installed Windows auto-start path stays in the tray without opening a
browser.

## Connect Your First Client

1. In **Accounts**, add an OpenCode Go account with an officially distributable
   API key. The login account is optional; when entered first, it is copied
   into the required display name until you edit that name yourself. The
   dashboard does not collect or manage an OpenCode login password.
2. In the dashboard's **Connection Center**, copy the **Key** and the
   **API Base URL** (`http://127.0.0.1:9042/v1`).
3. Point your client at the base URL with the Key. The
   **Applications** view has a per-client guide for 16 common tools.
4. Verify the setup with a real request.

The Key is the only credential a client needs, and it works in three
header forms: `Authorization: Bearer <key>`, the Anthropic-compatible
`x-api-key: <key>`, or the Gemini-compatible `x-goog-api-key: <key>`. It is a
local secret unrelated to the OpenCode-Go account key, which the gateway
retrieves from SQLite and injects on the upstream side itself.

Minimal POSIX-shell checks for all five client formats:

```bash
BASE=http://127.0.0.1:9042
KEY=replace-with-gateway-key

# OpenAI Chat Completions
curl "$BASE/v1/chat/completions" -H "Authorization: Bearer $KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"deepseek-v4-flash","messages":[{"role":"user","content":"ping"}],"stream":false}'

# OpenAI Responses
curl "$BASE/v1/responses" -H "Authorization: Bearer $KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"deepseek-v4-flash","input":"ping","store":false}'

# Anthropic Messages
curl "$BASE/v1/messages" -H "x-api-key: $KEY" \
  -H "anthropic-version: 2023-06-01" -H "Content-Type: application/json" \
  -d '{"model":"deepseek-v4-flash","max_tokens":16,"messages":[{"role":"user","content":"ping"}]}'

# Claude Desktop: the alias is rewritten to the model saved in the Applications view
curl "$BASE/claude-desktop/v1/messages" -H "x-api-key: $KEY" \
  -H "anthropic-version: 2023-06-01" -H "Content-Type: application/json" \
  -d '{"model":"claude-sonnet-4-6","max_tokens":16,"messages":[{"role":"user","content":"ping"}]}'

# Gemini generateContent
curl "$BASE/v1beta/models/deepseek-v4-flash:generateContent" \
  -H "x-goog-api-key: $KEY" -H "Content-Type: application/json" \
  -d '{"contents":[{"role":"user","parts":[{"text":"ping"}]}]}'
```

## Upgrade, Backup, Restore, And Uninstall

Download upgrades from the
[latest GitHub Release](https://github.com/klarkxy/opencode-go-mgr/releases/latest)
and verify them against `SHA256SUMS` from the same release:
`Get-FileHash <file> -Algorithm SHA256` on PowerShell, `shasum -a 256 <file>`
on macOS, or `sha256sum <file>` on Linux.

### Entering The Updater Channel From Version 1.4.1

Version 1.4.1 predates the signed in-app updater. Windows users enter the
updater channel once:

1. Choose **Quit** from the OCG Manager tray icon.
2. Run the first updater-enabled Windows setup.
3. On the upgrade-method page, select the second option, **Install without
   uninstalling** (不要卸载，直接安装), then continue. The first option is
   merely Tauri's default selection; it is not required.

Do not uninstall 1.4.1 first — the direct overwrite preserves the existing
data directory. An optional advanced equivalent:

```powershell
Start-Process -FilePath .\ocg-manager_<version>_windows-x64-setup.exe -ArgumentList '/UPDATE','/P','/R' -Wait
```

macOS and Linux users perform the direct replacement described below once.
After the first updater-enabled release is installed, future signed desktop
releases can be downloaded and installed from **Settings** with one action.
CLI and Docker upgrades remain manual.

### Multi-Key Upgrade And Downgrade Notes

Upgrading from a single-key version is seamless: your existing key remains
the **primary key** with its value unchanged, clients keep authenticating
with it, and a background task attributes historical forward logs to the
primary key (usage from before the upgrade is counted toward the primary key
as an approximation).

Sub keys you create after upgrading live in their own database table that
single-key builds never read or rewrite. Downgrading to such a build is
safe: the primary key value survives untouched, every sub key and its
enabled/disabled/deleted state is intact when you upgrade again, and no
revoked credential can ever come back to life by downgrading. Sub keys
simply do not authenticate while the older build runs.

### Plan Migration (Schema v23)

Schema v23 stores Plan verification state, Alias / upstream log identity
(`requested_model`, `resolved_alias`, `upstream_model`), optional native cost
(`native_cost_value`, `native_cost_unit`, `native_cost_currency`), Custom API
config tables, and SCNet risk acknowledgements. Before it writes a v23
database, the application creates one verified, non-overwriting rollback copy
beside `data.sqlite`, named `data.sqlite.pre-v23.<timestamp>.bak`. Keep that
file with your normal backup until the upgraded installation has been
verified. It is a pre-v23 rollback point, not a replacement for a complete
backup; never open the migrated database with an older build.

If the source database was older than schema v22, the same startup also keeps
`data.sqlite.pre-v22.<timestamp>.bak` as the earlier rollback point. On every
`Database::open`, enabled leftovers for Command Code GOAT and all three SCNet
Token Plan tiers are disabled without changing `updated_at`. Custom API
enabled state is preserved. Their verification and configuration remain
intact, except that an existing unverified GOAT row is reset to `pending`.
OpenCode Go, Zen Free, and unknown provider/offering pairs are untouched.

### Backup

1. Stop every process using the data: choose **Quit** from the desktop tray,
   stop the CLI with Ctrl+C or its service manager, or run
   `docker compose stop`.
2. Copy the **entire** GUI or CLI data directory. Desktop
   `browser-profiles/` is already inside the GUI data directory. For Docker,
   back up both sensitive volumes: `ocg-data` and `ocg-browser-profiles`.
   With the containers stopped, run
   `docker compose cp ocg-manager:/data/. ../ocg-data-backup` and
   `docker compose cp ocg-manager:/browser-profiles/. ../ocg-browser-profiles-backup`.
3. Keep the backup outside the repository, and check that it contains
   `data.sqlite` and, where present, `.encryption-key`. Browser profiles hold
   long-lived cookies and login state and are not encrypted by OCG Manager;
   protect them like account keys and the database.

### Restore

1. Stop the process, move the current data aside, and copy the whole backup
   back to its original directory or an empty Docker volume.
2. Start the same or a newer version.

Caveats:

- Docker files in `/data` must remain writable by UID/GID `10001`.
- Docker files in `/browser-profiles` must also remain writable by UID/GID
  `10001`.
- Windows GUI obfuscation is bound to the Windows user and machine, so its
  data cannot restore account keys or passwords on another machine — create
  fresh data there and re-enter the credentials.
- macOS/Linux GUI, CLI, and Docker restores must preserve `.encryption-key`
  or the explicitly supplied `--encryption-key` /
  `OCG_MANAGER_ENCRYPTION_KEY` value.
- There is no automatic downgrade compatibility guarantee; do not open a
  newer database with an older build.

### Docker Restore Into A Fresh Volume

First verify the backup and confirm that `.env` pins the intended same or
newer image. The `docker compose down -v` command below permanently deletes
all current named volumes; run it only after preserving both kinds of
persistent data separately:

```bash
docker compose down -v
docker compose run --rm --no-deps --user root \
  --cap-add CHOWN --cap-add DAC_OVERRIDE --cap-add FOWNER \
  --entrypoint sh \
  --volume ../ocg-data-backup:/backup/data:ro \
  --volume ../ocg-browser-profiles-backup:/backup/browser-profiles:ro \
  ocg-manager \
  -c 'cp -a /backup/data/. /data/ && \
      cp -a /backup/browser-profiles/. /browser-profiles/ && \
      chown -R 10001:10001 /data /browser-profiles && \
      find /data /browser-profiles -type d -exec chmod 700 {} + && \
      find /data /browser-profiles -type f -exec chmod 600 {} +'
docker compose --profile browser up -d --no-build
docker compose ps
```

If the original deployment used `OCG_MANAGER_ENCRYPTION_KEY`, put the same
secret back into `.env` before the restore. Keep the backup until the
dashboard, accounts, and a real gateway request have all been verified.

### Upgrade And Uninstall By Surface

The direct GUI steps are also the fallback when in-app update is unavailable.

- **Windows GUI:** quit the tray app, run the new installer, and choose
  **Install without uninstalling**. Uninstall from Windows **Installed
  apps**; the uninstaller asks whether to delete `%USERPROFILE%\.ocg-mgr`.
- **macOS GUI:** replace the app in **Applications** with the new DMG copy.
  Delete the app to uninstall; remove `~/.ocg-mgr` separately only when you
  also intend to delete the data.
- **Linux GUI:** install the new `.deb` over the old package, or replace the
  AppImage. Remove the package or AppImage to uninstall; data remains in
  `~/.ocg-mgr` until you delete it.
- **CLI:** replace the extracted package as a unit so the executable,
  `dist/`, and `LICENSE` stay together. Delete that package to uninstall;
  data remains in `~/.ocg-mgr-cli` or the custom `--data-dir`.
- **Docker:** after backing up, run `docker compose pull` followed by
  `docker compose up -d --no-build`. If the browser profile is enabled, use
  `docker compose --profile browser pull` followed by
  `docker compose --profile browser up -d --no-build` so both images are
  upgraded together. Pin `OCG_IMAGE` and `OCG_BROWSER_IMAGE` to full release tags
  for repeatable production deployments. `docker compose down` removes
  containers but keeps `ocg-data` and `ocg-browser-profiles`;
  `docker compose down -v` permanently deletes them and is only for an
  intentional reset after a verified two-volume backup. Selecting an older
  image does not roll back the database; restore
  the complete backup made by that older version when a database rollback is
  required.

## The Dashboard

The dashboard is a single-page Vue 3 application served by the gateway. The
left rail (or the horizontal app menu below 1024px) exposes seven views:
**Dashboard**, **Access Keys**, **Accounts**, **Pricing**, **Applications**,
**Logs**, and **Settings**. The top right of the header holds the theme
switcher, the language switcher, and the sign-out button.

The dashboard speaks ten languages: 简体中文, 繁體中文, English, 日本語,
한국어, Español, Français, Deutsch, Português (Brasil), and Русский. The
default is 简体中文. The choice persists in `localStorage` under
`ocg-manager.locale`; when persistence is unavailable (for example in a
private window), the in-memory locale still works for the current session.

### Connection Center

The first panel above the fold — and the only panel that always stays on
top — is the **Connection Center**. It contains:

- The **Key**, with regenerate, one-click copy, and a **Manage access keys**
  action that opens the Access Keys view. Regenerating invalidates only the
  selected key's previous value immediately; other keys keep working. When
  more than one enabled key exists, a selector lists them and switches the
  displayed (masked) value, the copy target, and the regenerate target.
  Copying places the full plaintext value on the clipboard — clear your
  clipboard history after use on shared or public computers. Create, rename,
  enable, disable, and delete live on **Access Keys**, not here. The primary
  key is reset with the same regenerate control as a sub key; there is no
  custom-value field.
- The **API Base URL** (e.g. `http://127.0.0.1:9042/v1`) with one-click copy,
  plus the full Chat Completions, Responses, and Messages endpoints.
- The **Upstream URL** the gateway forwards to, with a copy action.
- An **HTTP warning** that appears whenever the resolved root URL is a
  non-loopback `http://` URL, warning that the Key and request
  contents would be transmitted in clear text.

The **Downstream Access Root** setting in **Settings** controls only the URLs
the dashboard shows and the application tutorials emit. Its effective value
is selected in this order:

1. A non-empty `OCG_CLIENT_ROOT_URL` environment variable.
2. The manually saved dashboard value.
3. An automatic fallback: the current dashboard origin in production, or
   `http://127.0.0.1:<Gateway port>` in development.

While the environment variable is active, the input is read-only; changes
take effect after restart and are never written to SQLite. The automatic
value is shown in the input but is not saved.

Set an externally reachable root such as `https://ocg.example.com` when
clients reach the gateway through a reverse proxy or a different host. A
trailing `/v1` is accepted and removed automatically. This setting does
**not** change the gateway bind address, configure DNS, or create a reverse
proxy — those must already route to the running gateway. Plain HTTP is
allowed for LAN deployments, but it exposes the Key and request
contents to the network.

### Access Keys

The **Access Keys** view is the home for client-facing credentials:

- The **primary key** is always active and cannot be disabled or deleted; you
  rotate it with the reset control. It is the credential the application
  guides show by default. There is no field for typing a custom primary
  value.
- **Sub keys** are additional credentials you create, give a display name,
  rename, enable/disable, regenerate, or delete — handy for handing one key
  to each device. Deleting a sub key is a soft delete: it stops authenticating
  immediately and its plaintext is cleared, but forward logs keep resolving
  to its name. A sub key value may never equal the primary key value or
  another sub key value, and at most 64 non-deleted sub keys are
  supported.

The Connection Center and the Applications view only consume enabled keys.
Usage by key is filtered on the Logs view.

### Application Guides

The **Applications** view ships with per-client configuration snippets for 16
tools: Claude Code, Claude Desktop, Codex, Gemini CLI, Pi, Kimi Code CLI,
OpenCode, WorkBuddy, OpenClaw, Hermes, Cherry Studio, VS Code Copilot Chat,
Cline, Roo Code, Continue, and Chatbox. The connection panel shows the current
client's request URL, a Key selector (the primary Key plus enabled sub keys),
and model pickers. Node addresses and the upstream URL stay on Dashboard. Each
guide shows the protocol the tool speaks, the official documentation URL,
step-by-step instructions, and one or more editable code blocks with a
**Copy** button. The displayed block masks the Key; copying restores the real
key, so screenshots remain shareable without producing an unusable
configuration.

Before overwriting any existing configuration file, back up the original file. The code blocks
in Applications are editable, but keep a recoverable copy before copying or manually merging
their contents.

Base URL conventions per client:

- Claude Code, Cherry Studio, and Chatbox use the root URL without `/v1`.
- Claude Desktop uses that root plus `/claude-desktop`; its client then calls
  `/claude-desktop/v1/messages` and `/claude-desktop/v1/models`.
- Gemini CLI uses the root URL with `GOOGLE_GENAI_API_VERSION=v1beta`. Its
  remote Base URL must use HTTPS; only `localhost`, `127.0.0.1`, and `[::1]`
  may use HTTP. The Applications view disables Gemini configuration copying
  when the resolved root violates this client-side rule.
- Pi, Kimi Code CLI, OpenCode, OpenClaw, Hermes, Cline, Roo Code, and Continue
  use the API Base URL ending in `/v1`.
- VS Code Copilot Chat and WorkBuddy need the full `/v1/chat/completions` URL.
  Codex needs the API Base URL ending in `/v1` plus `wire_api = "responses"`.
  Use `~/.codex/ocg.config.toml` with `codex --profile ocg` (CLI) or merge the
  same provider block into user-level `~/.codex/config.toml` (Desktop / default
  provider). `~/.codex/ocg-model-catalog.json` is optional: skip it to connect.
  Enable `model_catalog_json` only for the picker plus real context windows and
  reasoning levels. A catalog replaces Codex's bundled model list and must
  include the current required fields. Without a catalog, unknown slugs use
  Codex's 272K fallback metadata. Requests always use OCG Manager's Responses
  endpoint.

Codex's `~/.codex/ocg-model-catalog.json`, `~/.codex/ocg.config.toml`, and
`~/.codex/config.toml` are local configuration files. Back up each file before overwriting
or merging it. When using CC Switch proxy mode, back up the configuration directory saved by
CC Switch separately; do not mix the direct OCG configuration with the proxy configuration.

The Applications picker list is the protected
`GET /dashboard/api/application-models` response: currently routeable OpenCode
Go aliases intersected with the active OpenCode Go pricing snapshot. Highspeed
variants inherit the base model's pricing row. An empty intersection is `[]`,
not an error. That list is **not** the same as authenticated `GET /v1/models`,
which publishes currently routeable Go and Zen Free aliases plus eligible
Custom declared IDs. `application-models` stays Go-only. Both endpoints are
local lists: no SCNet official table spellings, no unpublished Command Code
GOAT names, no upstream discovery, and no account selection to fetch a
catalog. An accepted pricing refresh can
change which Go aliases `application-models` returns. The view reloads this
local list whenever you return to it. Model selections and edited snippets are
cached separately per application while the current dashboard page remains
alive; a page reload resets this in-memory state. **Restore defaults** resets
the active application's model selection and snippet drafts.

### Model capabilities

Application snippets use the verified limits below
(`src/views/application-guides.ts`, 2026-08-14). Input is what OCG can
actually carry. The passthrough / conversion matrix is under
[Protocol Conversion](#protocol-conversion).

| Model | Context | Output | Input | Reasoning | Tools | Efforts |
| --- | ---: | ---: | --- | --- | :---: | --- |
| `grok-4.5` | 500K | 500K | text, image | always | ✓ | low / medium / high (default high) |
| `gpt-5.6-luna` | 1.05M | 128K | text, image | ✓ | ✓ | low / medium / high / max (default medium) |
| `muse-spark-1.2` | 1M | 128K | text, image | ✓ | ✓ | low / medium / high (default high) |
| `muse-spark-1.2-contributor` | 1M | 128K | text, image | ✓ | ✓ | low / medium / high (default high) |
| `glm-5.3` | 1M | 128K | text | ✓ | ✓ | low / high / max (default max) |
| `glm-5.2` | 1M | 128K | text | ✓ | ✓ | high / max (default max) |
| `glm-5.1` | 198K | 32K | text | ✓ | ✓ | — |
| `kimi-k3` | 1M | 128K | text, image, video | always | ✓ | max |
| `kimi-k2.7-code` | 256K | 256K | text, image, video | always | ✓ | — |
| `kimi-k2.6` | 256K | 64K | text, image, video | ✓ | ✓ | — |
| `mimo-v2.5` | 1M | 128K | text, image, audio, video | ✓ | ✓ | — |
| `mimo-v2.5-pro` | 1M | 128K | text | ✓ | ✓ | — |
| `minimax-m3` | 1M | 128K | text, image | ✓ | ✓ | — |
| `minimax-m2.7` | 200K | 128K | text | always | ✓ | — |
| `minimax-m2.7-highspeed` | 200K | 128K | text | always | ✓ | — |
| `minimax-m2.5` | 200K | 64K | text | always | ✓ | — |
| `minimax-m2.5-highspeed` | 200K | 64K | text | always | ✓ | — |
| `qwen3.8-max` | 1M | 128K | text | ✓ | ✓ | — |
| `qwen3.7-max` | 1M | 64K | text | ✓ | ✓ | — |
| `qwen3.7-plus` | 1M | 64K | text, image | ✓ | ✓ | — |
| `qwen3.6-plus` | 1M | 64K | text, image | ✓ | ✓ | — |
| `deepseek-v4-pro` | 1M | 384K | text | ✓ | ✓ | high / max (default high) |
| `deepseek-v4-flash` | 1M | 384K | text | ✓ | ✓ | high / max (default high) |
| `hy3` | 256K | 64K | text | ✓ | ✓ | low / high (default high) |

`muse-spark-1.2` uses Zero Data Retention (ZDR): prompts and completions are
not used for training. `muse-spark-1.2-contributor` is not ZDR; prompts and
completions may be used for training. Select Contributor only for data you are
authorized to use this way. The standard Muse price is measured from live Go
usage because the public Go pricing table lists only Contributor.

Rounded display: 198K = 202,752; 200K = 204,800; 256K = 262,144; 1M = 1,000,000
or 1,048,576. `glm-5.3` limits and token rates follow the dedicated
models.dev `opencode-go/glm-5.3` row (same $1.40 / $4.40 / $0.26 as the
official Go table; Usage remains $15).

Claude Desktop is the exception with durable model mappings: before its
configuration is copied, the selected `sonnet`, `opus`, and `haiku` targets
are saved to SQLite through the protected dashboard API. Omitted roles
inherit the first configured role, and the three roles cannot all be empty.
Its restore action returns to the mapping loaded or last saved in the current
page.

### Accounts

Each account card binds one **Plan** (provider + offering), and when that Plan
requires one, one credential, plus one independent quota pool. Cards share one
manually persisted global order;
after the request's capability filter, strict priority, global sticky, and
round-robin routing all reuse that order. There is no separate provider page,
model-routing page, or per-model quota pool. The seven dashboard views stay
**Dashboard**, **Access Keys**, **Accounts**, **Pricing**, **Applications**,
**Logs**, and **Settings**.

The built-in Plan families are:

| Family | Plan | Live routing | Notes |
| --- | --- | --- | --- |
| OpenCode Go | `opencode` / `go` | Yes | One officially distributable API key per card; managed signup remains Beta |
| Zen Free | `opencode-zen-free` / `anonymous-free` | Yes | One credentialless, anonymous singleton; sortable and enableable, not deletable; quota shared by egress IP |
| Command Code GOAT | `command-code` / `goat` | No | Saved as a disabled `pending` draft; connection verification returns `501`; not a production inference, pricing, usage, or provider-guide path |
| SCNet Token Plans | `scnet` / `token-plan-basic`, `token-plan-standard`, `token-plan-premium` | No | Keys must start with `sk-tp-`; saved as disabled `pending` drafts; verification returns `501`; official interactive-use restriction below |
| Custom API | `custom` / `api` | Yes | Trusted-administrator destination; create/update stay disabled `pending`; verify then explicit enable; eligible declared IDs appear on `/v1/models`; unpriced/unknown cost, no quota debit |

Every persistent mutation path (Database, dashboard, CLI, and Tauri) rejects
`enabled=true` for a catalogued `routable=false` offering (GOAT and all SCNet
tiers) before it mutates the row, revision, or timestamps. Custom is
catalog-routable, but create/update still leave the card disabled and
`pending`; enable is rejected until verification status is `verified`.
Disabled drafts remain saveable.

For OpenCode Go, Command Code GOAT, and SCNet Token Plans, do not add consumer
subscription credentials, browser cookies, or reverse-proxy credentials as
account keys. This restriction does not constrain administrator-configured
Custom Bearer or `x-api-key` credentials. Command Code GOAT and SCNet Token
Plans must not be
aliased onto OpenCode, must not send their keys to an OpenCode endpoint, and
must not be described as live routing, usage, pricing, or verification.
Custom API must not be aliased onto OpenCode or send its key to an OpenCode
endpoint; it is a separate trusted-administrator destination, not an
OpenCode offering.

SCNet Token Plan keys (`sk-tp-`) are limited to interactive use inside AI
tools. Account sharing and using the API as a custom application backend,
automation script, or non-interactive batch caller is prohibited and may
suspend the subscription or revoke the key. Saving a draft records a
versioned acknowledgement of that restriction (`scnet-token-plan-restrictions`
/ `2026-08-21`); the acknowledgement itself does not enable routing. The
official usable-model table and endpoint snapshot are adapter input only and
are never published as client aliases.

Custom API is a live trusted-administrator destination. The card stores a
base URL, one upstream protocol (Chat Completions, Responses, or Messages),
one auth scheme (Bearer or `x-api-key`), and at least one model capability.
A trusted administrator may configure any syntactically valid HTTP or HTTPS
origin, including LAN, loopback, and other self-selected destinations.
URL-embedded credentials, query strings, and fragments are rejected. The
gateway never follows redirects, never forwards dashboard or client
authentication, and constructs only the configured Bearer or `x-api-key`
credential. Joined endpoints stay inside the configured scheme, host, port,
and base-path prefix. Custom HTTP uses the same process-wide Direct / Manual
/ Auto proxy policy; connect and request timeouts are bounded from the
configured connect timeout (clamped 5–60 seconds).

Create and update leave the card disabled and `pending`. Verification sends
one protocol-correct, non-stream, token-bounded JSON request to the first
declared model; only a `2xx` JSON object succeeds. Verification does not
discover or mutate capabilities and never auto-enables the account. You must
enable the card explicitly after a successful verify. Eligible accounts
(enabled + verified + ready + non-empty key) expose their declared model IDs
on authenticated `GET /v1/models` and can be selected for those IDs.
Declared capability IDs are both the client-facing names and the upstream
model names; matching is case-insensitive for kebab IDs, and names with `/`,
`_`, or whitespace never fold onto a kebab alias. Custom overlay never
steals a published Go or Zen Free alias. Overlap with another Plan's unique
raw ID returns `ambiguous_model_id` and does not call upstream. Undeclared
names stay unknown (`400`). Changing the base URL, key, or declared
capabilities re-pends verification and disables the account. Upstream
protocol and auth scheme are fixed at create. Custom traffic is unpriced:
logs record `cost_state=unknown` with no quota debit, and Custom has no
provider usage refresh. `MODEL_PROTOCOLS` remains Go-specific; Custom
converts the client protocol to the account's declared upstream protocol.

The **Accounts** view splits creation into **Import existing Key** and
**Register new account (Beta)**:

- A **Key account** stores one officially distributable OpenCode Go API key.
- A **managed account** immediately creates a disabled, recoverable draft,
  then runs the wizard through optional sign-in identity, invite registration,
  payment, and key verification. The draft and current step are persisted to
  SQLite, so closing the page or restarting the service does not lose the
  flow. Pending accounts cannot be selected by the gateway and do not expose
  usage, test, or enable controls.

Managed signup and isolated browser profiles are **Beta** features. They have
not been thoroughly tested; do not rely on them in production.

When you create a managed draft, the form shows the **invite URL** (prefilled
from Settings; fresh installs may ship a demo default). Edit it in place: it
must be an HTTPS URL no longer than 2,048 characters, contain no username or
password, and use exactly `opencode.ai` or `console.opencode.ai` as its host.
If it differs from Settings, it is written back to **Settings → OpenCode Go
invite URL**. Changes affect later invite-page opens only; they do not rewrite
completed accounts. Replace the demo default with your own invite link before
a real signup, or referral credit goes to the link owner.

The managed wizard is intentionally manual (no password autofill, no payment
clicks, no automatic key extraction):

1. **Sign-in identity (optional).** Sign up for Google or GitHub only if you
   need a new account; otherwise **skip this step**. OpenCode sign-in can also
   finish on the next step.
2. **Invite registration.** Open the invite URL in the same isolated profile
   and complete OpenCode sign-in/registration with Google or GitHub.
3. **Payment.** Confirm the plan and amount in the console; only you complete
   payment on the page.
4. **Verify Key.** Copy the key from the console, paste it, and run a real
   upstream probe.

Click an earlier finished step in the step bar to **rewind**; forward progress
still uses each step's primary button. A `2xx` verification completes and
enables the account. A `429` also proves that the key is valid, completes the
account, and records the current cooldown. `401`/`403`, network errors, and
`5xx` responses leave the account at key verification so you can correct it
and retry.

Every account has a durable, isolated browser profile. Desktop builds launch
an external Chromium-family browser: Windows prefers Edge and then Chrome;
macOS checks Chrome, Edge, and Chromium; Linux desktop searches `PATH` for
Chrome, Chromium, or Edge. It uses only
`browser-profiles/<account_id>`, first-run suppression, and a new window; it
does not enable CDP, automation, `--no-sandbox`, or weakened web security.
Older `profiles/<account_id>` WebView data is deliberately not imported, so
the first open after upgrading requires another login.

Every completed account has **Open OpenCode console**
(`https://opencode.ai/auth`). A legacy account starts with a blank isolated
profile the first time; sign in once and its cookies remain available.
Google/GitHub and OpenCode cookies belong to different domains, but both stay
in the same account profile.

Resetting browser identity first closes that account's browser and removes
both new and legacy profile directories. A completed account keeps its key
and is only signed out of the console; a pending managed account also returns
to the sign-in identity step. Deleting an account likewise deletes its
cookies/profile, and the confirmation states this explicitly. That login state
can then be recovered only from a backup or by signing in again.

Each completed OpenCode Go card shows the account name, cooldown state, and
the 5-hour / weekly / monthly usage bars driven by local accounting. Zen Free
has its own anonymous, egress-IP-shared free cooldown rather than a key quota.

- **Usage baselines.** Type a percentage or drag a bar to set its current
  real-world usage baseline. After the value is saved, successful request
  cost recorded by OCG Manager continues to accumulate above that baseline.
  Reaching 100% is still only a warning; it does not stop the gateway from
  selecting the account. Manual calibration stays available for every ready
  account.
- **Refresh quota (ready Key and managed accounts).** Official OpenCode usage
  (`/zen/go/v1/usage`) is a periodic calibration baseline; local forward-log
  costs on this node remain the immediate estimator after the last successful
  sync. Ready and enabled accounts with local activity in the last 24 hours are
  reconciled about hourly; inactive ready accounts about daily. Disabled,
  unfinished, or empty-key accounts are never auto-refreshed. Opening the
  Accounts page does not trigger a fetch. Gateway startup does not fetch
  immediately: eligible accounts without a saved schedule are spread across
  the first 0–15 minutes, then follow the normal cadence.
  Clicking **Refresh quota** still runs the same secure path on demand, with a
  server-side 60-second per-account throttle (Retry-After / next-allowed). The
  card shows the last successful official sync time and any temporary retry
  wait—not only a button spinner. Local estimates that reach ≥80% may get an
  expedited sync at most once per 15 minutes. A real inference `429` still
  writes the existing cooldown/selector state and additionally schedules an
  official reconciliation about 1–2 minutes later; official failures or
  `status=rate-limited` never write inference cooldown. Failures keep the
  previous baseline and last-success timestamp. The request uses the same
  global outbound proxy as other dashboard fetches.
- **Identity and credentials.** The name is the account's required primary
  display label. The login account field is optional; on Key-account creation,
  entering it first copies it into the name until you edit the name yourself.
  Optional freeform notes live in **Edit account**. They can stay empty and
  do not affect routing or quota. The dashboard stores the account key but
  does not collect or manage third-party login passwords.
- **Purchase date.** New accounts default to the browser's current date, and
  the value remains editable. The managed wizard also writes the purchase date
  when payment advances to key verification. Expiry is the same day in the
  next natural month, clamped to that month's last day when necessary:
  `2026-01-31` expires on `2026-02-28`. Accounts and Dashboard show days
  remaining, due today, or days expired. This is informational only and never
  disables an account or prevents the gateway from selecting it.
- **Priority order.** Use the drag handle on an account card to persist its
  priority with a mouse, touchscreen, or pen. When the handle has keyboard
  focus, the Up and Down arrow keys move the account as well. Dashboard, the
  Logs account filter, CLI listings, and the gateway selector all consume
  this same SQLite-backed order.
- **Cooldown reset.** You can reset a cooldown manually from this view. The
  bar snaps back to its local estimate as soon as the cooldown is cleared.

### Pricing

The **Pricing** view shows immutable provider pricing snapshots. Refresh is
manual only: OpenCode Go can fetch `https://opencode.ai/docs/go/` after you
press refresh, while offerings without a verified first-party pricing contract
remain unavailable and cannot be refreshed. A failed fetch or validation keeps
the last successful snapshot.

For OpenCode Go, the view shows the revision, documentation timestamp, window
limits, token rates, `Usage`, and the quota-debit multiplier. The allowance is
not a quota pool and does not route requests: it only derives that debit
multiplier (`monthly limit / Usage`). Saving a temporary override creates a
new persistent revision for later estimates. There is no model-level quota
pool. Command Code GOAT and SCNet Token Plans have no live pricing or usage
path. Custom API is catalogued as unpriced: successful forwards log
`cost_state=unknown` with no quota debit and no official usage refresh.

### Logs

The **Logs** view shows the rolling buffer of requests the gateway has
forwarded: timestamp, selected provider/offering, route account, credential
account, model, status code, the upstream error if any, and the streamed usage
when the upstream emitted a usage chunk. Filters cover provider, offering,
route account, credential account, model, status, time range, and client Key.
Each stored row keeps the request identity separate from the upstream
identity. There is no `requested_alias` field:

- `requested_model` — the alias or model name the client sent
- `resolved_alias` — the canonical kebab alias when one exists
- `upstream_model` — the Plan's raw upstream ID

plus `provider_id` and `offering_id`. The existing model filter exact-matches
any of those identities or the legacy `model` column. Native cost
(`native_cost_value`, `native_cost_unit`, `native_cost_currency`) is optional
and present only when the offering supplies enough pricing evidence.

Each row also preserves raw supplier cost, quota debit, and effective paid cost
when the selected offering supplies enough pricing evidence. These are distinct
values: an allowance only changes the quota-debit multiplier; it does not make
a model or provider routable.

- Chat streaming requests set `stream_options.include_usage` so OpenAI-compatible
  upstreams emit a usage chunk. Rows with `success_no_usage` mean the stream
  still finished without one. A usage chunk makes token counts accurate; quota
  use is still estimated from the active OpenCode Go pricing snapshot. Zen free
  models (`*-free`, `big-pickle`) record tokens with `cost_state=free` and do
  not enter Go quota totals. Custom API rows record `cost_state=unknown` with
  no quota debit. Expand a row to see the request ID and diagnostic
  detail.
- An `outcome_unknown` row means the upstream may already have completed and
  charged the request, but the gateway lost the response or timed out. Such a
  request is not replayed automatically and its local cost remains unknown.
- The **Key** filter narrows rows and the summary totals to one client key.
  Its options come from the log table itself, so disabled, deleted, and
  otherwise unknown keys stay filterable. **Unattributed** selects rows
  written before multi-key support; a background task attributes those to
  the primary key after upgrading, so usage from before the upgrade is
  counted toward the primary key as an approximation.

### Settings

The **Settings** view exposes the persistent gateway configuration:

- **Gateway Port** — the port the gateway binds (default `9042`).
- **Upstream URL** — the OpenCode-Go base URL.
- **Routing mode** — strict priority, global sticky, or round robin. All three
  modes apply the one global card order only after filtering incompatible,
  disabled, cooling, or already-failed cards; they do not create a provider or
  model routing table.
- **Outbound proxy** — a process-wide setting shared by every account.
  `Automatic (system / environment)` reads `HTTP_PROXY`, `HTTPS_PROXY`,
  `ALL_PROXY`, and `NO_PROXY`; Windows also reads the system proxy and connects
  directly when none is configured. `Manual HTTP proxy` strictly routes all
  HTTP/HTTPS targets through one `http://` or `https://` proxy such as
  `http://127.0.0.1:7890`; a proxy failure never silently falls back to a direct
  connection. `Force direct connection` ignores system and environment proxy
  configuration. Proxy URLs cannot contain credentials. The policy covers core
  HTTP requests including model forwarding (OpenCode Go, Zen Free, and Custom
  API), account-key tests and Custom verification, official OpenCode Go usage
  API, pricing refreshes, release checks, and signed desktop installer
  downloads; authenticated `GET /v1/models` and protected
  `GET /dashboard/api/application-models` are local lists and do not use
  this outbound path. The browser sidecar is outside its scope. **Test
  connection** uses the unsaved form values against the current upstream. Any
  HTTP status proves network reachability, without running model inference or
  incurring model usage.
- **OpenCode Go invite URL** — the restricted HTTPS invite used by managed
  account onboarding. Fresh installs may ship a demo default; replace it with
  your own link before a real signup. Creating a managed draft can also edit
  and write this value back.
- **Downstream Access Root** — see [Connection Center](#connection-center).
- **Auto-start on login** — only the installed Windows desktop build exposes
  this switch. Development builds, the CLI, Docker, macOS, and Linux
  dashboards hide it.
- **Dock icon** — only the macOS desktop build exposes this switch. Turning
  it off keeps the menu-bar icon available. Windows, Linux, CLI, and Docker
  dashboards hide it.
- **Connect / non-stream / stream-idle timeouts** — default to 30, 900, and
  300 seconds. The non-stream value is a whole-request deadline; the stream
  idle value is enforced between response chunks. Existing installations are
  migrated from 30/120/300 only when that complete old default tuple is still
  untouched.
- **Check for updates / Update now** — updater-enabled installed desktop
  builds check the latest GitHub Release and can download, verify, and
  install its signed platform package. Version 1.4.1 needs the one-time
  direct overwrite install described above. Development builds, the CLI, and
  Docker keep the release-link/manual-upgrade path. The host must be able to
  reach GitHub; a failed check or install does not affect gateway forwarding.
- **Free model routing** — three OpenCode Zen modes (`deny` / `explicit` /
  `prefer`). See [Free model policy](#free-model-policy).

Configuration settings are written to SQLite and reloaded on the next start.
The update check is an on-demand action and is not persisted.

## Gateway Behavior

### Endpoints

The gateway is served at `http://<bind>:<port>` and exposes:

| Method | Path | Purpose |
| --- | --- | --- |
| `POST` | `/v1/chat/completions` | OpenAI Chat Completions |
| `POST` | `/v1/responses` | OpenAI Responses |
| `POST` | `/v1/messages` | Anthropic Messages |
| `GET`  | `/v1/models` | Authenticated local list: published Go/Zen aliases plus eligible Custom IDs (no upstream discovery) |
| `POST` | `/v1beta/models/{model}:generateContent` | Gemini non-stream generation (`/v1/models/...` is also accepted) |
| `POST` | `/v1beta/models/{model}:streamGenerateContent` | Gemini SSE generation (`/v1/models/...` is also accepted) |
| `POST` | `/v1beta/models/{model}:countTokens` | Returns `501`; Gemini CLI can fall back to local estimation |
| `POST` | `/v1beta/models/{model}:embedContent` | Returns `501`; embeddings are not supported |
| `GET`  | `/claude-desktop/v1/models` | Claude Desktop alias model list |
| `POST` | `/claude-desktop/v1/messages` | Claude Desktop Messages with alias rewriting |
| `GET`  | `/dashboard/` | Vue 3 dashboard (HTML) |
| `*`    | `/dashboard/api/...` | Dashboard JSON API |

The default bind is `127.0.0.1:9042`. The CLI can override the host with
`serve --host 0.0.0.0` and the port with `serve --port <port>`. The desktop
app also binds loopback and uses a Tauri single-instance lock to prevent two
tray apps from competing for the port. There is no HTTP health endpoint;
Docker checks container-internal TCP port `9042`.

### Authentication

Gateway API endpoints require the **Key** in one of three header
forms: `Authorization: Bearer <key>`, `x-api-key: <key>`, or
`x-goog-api-key: <key>`. Before forwarding, the gateway strips the client
auth header and injects the selected account credential instead. OpenCode Go
uses `x-api-key` for Messages upstreams and `Authorization: Bearer` for Chat
Completions / Responses. Custom API constructs only the configured Bearer or
`x-api-key` header and never forwards dashboard or client credentials.

Dashboard authentication depends on the listener bind:

- **Loopback binds (the default).** Requests that come straight to the
  loopback address skip dashboard login unless they carry `Forwarded`,
  `x-forwarded-for`, `x-forwarded-proto`, or `x-real-ip`; any of those
  headers requires login. The client still needs the **Key** to reach
  the upstream endpoints. This is what the desktop app and the default CLI
  use.
- **Non-loopback binds.** A single administrator account, stored as an
  Argon2 password hash in SQLite, governs the dashboard. Sign-in returns an
  HttpOnly session cookie. Standard reverse-proxy forwarding headers on a
  non-loopback bind still require the cookie. In Docker, the first
  administrator can be bootstrapped with `OCG_ADMIN_USERNAME` and
  `OCG_ADMIN_PASSWORD`; otherwise the first registration wins.

### Aliases

Clients should send **aliases**: stable lowercase kebab-case names from the
local registry. Existing OpenCode Go model IDs are the preferred aliases;
case-folded spellings such as `GLM-5.2` are accepted.

Authenticated `GET /v1/models` lists currently routeable published aliases
(OpenCode Go and Zen Free) in deterministic registry order, then appends
eligible Custom capability IDs that do not match those aliases (`owned_by` is
`custom`). It does **not** discover, proxy, or cache an upstream catalog,
write a forward log, or mutate routing state. Published Go and Zen Free
aliases do not depend on whether any Go account exists. Eligible Custom IDs
come from enabled + verified + ready Custom accounts that have a key.

Protected `GET /dashboard/api/application-models` is a different local list:
currently routeable OpenCode Go aliases intersected with the active OpenCode
Go pricing snapshot. Highspeed variants inherit the base row. Empty
intersection is `[]`. It never includes Custom IDs, never selects an account,
and never calls upstream.

Neither list advertises SCNet official usable-model spellings or unpublished
Command Code GOAT names. Eligible Custom declared IDs may appear on
`/v1/models` even when they contain `/`; they are not folded onto kebab
aliases.

A raw upstream ID with exactly one registry mapping is pinned to that mapping
(no cross-Plan fallback or Zen prefer overlay); routability is checked
afterward. An unroutable mapping is therefore recognized but cannot produce a
production route. Names containing `/`, `_`, or whitespace are treated as raw
IDs and never folded onto a kebab alias (`glm/5.2` is not `glm-5.2`). A raw ID
that matches more than one mapping, including an eligible Custom capability
and another Plan, returns `400` with code `ambiguous_model_id` and does not
call upstream. Unknown names — not a published alias and not an eligible
Custom ID — return `400` on every supported client format: Chat Completions,
Responses, Messages, and Gemini `generateContent` / `streamGenerateContent`.
The published kebab alias `deepseek-v4-flash` stays Go-owned; the unique raw
ID `deepseek/deepseek-v4-flash` pins to Command Code GOAT and is not
production-selectable unless a colliding eligible Custom ID makes the name
ambiguous instead.

Forward logs keep the request identity separate from the upstream identity.
There is no `requested_alias` field:

- `requested_model` — the alias or model name the client sent
- `resolved_alias` — the canonical kebab alias when one exists
- `upstream_model` — the Plan's raw upstream ID

plus `provider_id` and `offering_id`. Native cost fields are optional.

Claude Desktop remains a separate three-role alias layer
(`claude-sonnet-4-6`, `claude-opus-4-6`, and `claude-haiku-4-5-20251001`)
rewritten to the mapping saved in **Applications** before Alias resolution.
`GET /claude-desktop/v1/models` still advertises only those three role
aliases, not the Plan model union.

### Protocol Conversion

Each known OpenCode Go model has a hardcoded **preferred** protocol and a
**supported** set (maintained after test-account probes; not discovered at
request time). When the client protocol is supported, the gateway passthroughs
the request and response. Otherwise it converts the **request body** to the
preferred upstream protocol and the **response body** (or SSE stream) back to
the client protocol. `MODEL_PROTOCOLS` remains Go-specific. Custom API
converts the client protocol to that account's declared upstream protocol. Conversion covers text, system instructions, images, tool
calls and tool results, reasoning content, completion status, errors, and
usage fields. Example: `glm-5.2` passthroughs Chat Completions, Responses, and
Messages; `grok-4.5` is Responses-only and converts Chat / Messages / Gemini
entries to Responses; `gpt-5.6-luna` prefers Responses and also passthroughs
Chat; `glm-5.3` is Chat-only.

| Preferred upstream | Models |
| --- | --- |
| OpenAI Chat Completions | `glm-5.3`, `glm-5.2`, `glm-5.1`, `glm-5`, `kimi-k3`, `kimi-k2.7-code`, `kimi-k2.6`, `kimi-k2.5`, `deepseek-v4-pro`, `deepseek-v4-flash`, `mimo-v2.5`, `mimo-v2.5-pro`, `hy3` |
| OpenAI Responses | `grok-4.5`, `gpt-5.6-luna`, `muse-spark-1.2`, `muse-spark-1.2-contributor` |
| Anthropic Messages | `minimax-m3`, `minimax-m2.7`, `minimax-m2.7-highspeed`, `minimax-m2.5`, `minimax-m2.5-highspeed`, `qwen3.8-max`, `qwen3.7-max`, `qwen3.7-plus`, `qwen3.6-plus`, `qwen3.5-plus` |

Passthrough matrix (live test-account probe, 2026-08-14). ✓ = client protocol
is forwarded as-is; empty = converted to the model's preferred protocol.
Source of truth: `MODEL_PROTOCOLS` in
`crates/ocg-core/src/gateway/protocol.rs`.

`reasoning.effort` aliases (applied before forwarding or conversion):
`muse-spark-1.2` and `muse-spark-1.2-contributor` map `max` → `xhigh`
(upstream rejects `max`). Other models pass `reasoning.effort` through
unchanged.

| Model | Preferred | Chat | Responses | Messages |
| --- | --- | :---: | :---: | :---: |
| `grok-4.5` | Responses | | ✓ | |
| `glm-5.3` | Chat | ✓ | | |
| `glm-5.2` | Chat | ✓ | ✓ | ✓ |
| `glm-5.1` | Chat | ✓ | ✓ | ✓ |
| `glm-5` | Chat | ✓ | ✓ | ✓ |
| `gpt-5.6-luna` | Responses | ✓ | ✓ | |
| `muse-spark-1.2` | Responses | | ✓ | |
| `muse-spark-1.2-contributor` | Responses | | ✓ | |
| `kimi-k3` | Chat | ✓ | | ✓ |
| `kimi-k2.7-code` | Chat | ✓ | | |
| `kimi-k2.6` | Chat | ✓ | | |
| `kimi-k2.5` | Chat | ✓ | | |
| `deepseek-v4-pro` | Chat | ✓ | ✓ | ✓ |
| `deepseek-v4-flash` | Chat | ✓ | ✓ | ✓ |
| `mimo-v2.5` | Chat | ✓ | | |
| `mimo-v2.5-pro` | Chat | ✓ | | |
| `hy3` | Chat | ✓ | | |
| `minimax-m3` | Messages | ✓ | | ✓ |
| `minimax-m2.7` | Messages | ✓ | | ✓ |
| `minimax-m2.7-highspeed` | Messages | ✓ | | ✓ |
| `minimax-m2.5` | Messages | ✓ | | ✓ |
| `minimax-m2.5-highspeed` | Messages | ✓ | | ✓ |
| `qwen3.8-max` | Messages | ✓ | | ✓ |
| `qwen3.7-max` | Messages | ✓ | | ✓ |
| `qwen3.7-plus` | Messages | ✓ | | ✓ |
| `qwen3.6-plus` | Messages | ✓ | | ✓ |
| `qwen3.5-plus` | Messages | ✓ | | ✓ |

Unknown model names are rejected with `400` on every supported client format
(Chat Completions, Responses, Messages, and Gemini `generateContent` /
`streamGenerateContent`). Unknown Claude Desktop aliases are also `400`. The
gateway will not guess a protocol by trial because that could double-bill the
request. See [Aliases](#aliases).

Gateway protocol endpoints accept JSON request bodies up to 16 MiB. This
transport limit is separate from each model's context window. If a reverse
proxy sits in front of OCG Manager, configure it to allow request bodies of
at least 16 MiB or it may return `413 Payload Too Large` before the gateway
sees the request.

#### Responses is stateless

The following fields return `400` instead of being silently ignored:

- `previous_response_id`
- `conversation`
- `store: true` or any `store` value other than `false`
- `background: true`
- `input_image.file_id` (the gateway has no Files API)

Function, custom, and namespace tools convert normally. Hosted tools such as
`web_search`, `web_search_preview`, and `tool_search` cannot run on
OpenCode-Go; their declarations are dropped in automatic tool mode, and
forcing one returns `400`.

#### Gemini is a client-only format

The gateway never sends Gemini wire data upstream. It converts `contents`,
text-only `systemInstruction`, supported `inlineData` images,
`functionDeclarations`, function calls/results, JSON-schema output,
generation options, Google error envelopes, usage metadata, and SSE frames to
and from the known model's native Chat Completions or Messages protocol. Both
the `v1beta` and `v1` URL forms are accepted.

The compatibility boundary — nothing is silently pretended equivalent:

- Non-empty `safetySettings` return `400 INVALID_ARGUMENT`, because a
  different upstream protocol cannot preserve their safety semantics.
  Omitted, `null`, and `[]` are accepted. Do not treat `safetySettings` as a
  hint the upstream will enforce.
- `generationConfig.topK` and `generationConfig.thinkingConfig` are accepted
  as cross-protocol compatibility hints only; sampling, reasoning budgets,
  and thought display are not guaranteed equivalent to a native Gemini
  backend and depend on the selected OpenCode-Go model.
- Other non-null generation options that cannot be preserved — including
  `seed`, presence/frequency penalties, log-probability controls, and media
  resolution — return `400` instead of being silently discarded.
- `cachedContent`, `fileData`, Google Search, URL Context, Code Execution,
  multimodal function-response parts, function response schemas/behavior,
  `VALIDATED` function calling, candidate counts other than one, and response
  modalities other than `TEXT` return `400`. Use base64 `inlineData` for PNG,
  JPEG, GIF, or WebP images.
- `countTokens` and `embedContent` return `501 UNIMPLEMENTED`; Gemini CLI can
  fall back to local token estimation, and the gateway has no embeddings
  route.

#### Claude Desktop aliases

The dedicated entry accepts only the advertised aliases
`claude-sonnet-4-6`, `claude-opus-4-6`, and `claude-haiku-4-5-20251001`.
Before entering the existing Messages conversion path, the gateway rewrites
the alias to the actual model saved from the Applications view; model
capabilities, tool support, and context limits in the response still follow
the actual model. The `sonnet`, `opus`, and `haiku` mappings are serialized
inside `AppConfig`; omitted roles inherit the first configured role, while
the dashboard returns the resolved three-role mapping.

### Account Selection And Failover

Accounts are tried in **list order**, which can be reordered and persisted
from the Accounts view. The selector skips:

- Disabled accounts.
- Accounts that are cooling down.
- Accounts that have already failed during the current request (e.g. with a
  `429`).

A `429` with a recognized `Resets in …` phrase writes `cooldown_until` and
the gateway tries the next account. `401` and `403` responses fail over
without writing a cooldown — they are an authentication problem, not a quota
problem. A DNS/TCP/TLS connection failure that proves the request was not
sent is retried once on the same account, including for streaming calls.

The gateway does not replay `408`, `5xx`, post-connect transport failures,
response-body timeouts, or interrupted streams. Ambiguous failures are
reported as `upstream_outcome_unknown` and logged as `outcome_unknown`,
because the upstream may already have consumed quota. When every enabled
account is cooling down, the gateway returns `429` with the soonest reset
time.

### Cost Accounting

The 5-hour, weekly, and monthly bars are local estimates, driven by the
requests the gateway actually forwards — not by the upstream's authoritative
billing. Token rates, window limits, and each model's `Usage` come from the
active OpenCode Go USD snapshot.

- The official multiplier defaults to `monthly limit / Usage`. A user can
  override it for a temporary promotion; subsequent requests use the active
  persisted value, and refresh never overwrites it without confirmation.
- `deepseek-v4-pro` (DS V4 Pro), `deepseek-v4-flash`, `mimo-v2.5-pro`, and Grok
  currently have a `$15` Usage allowance, which corresponds to a
  `60 / 15 = 4x` multiplier.
- The applicable local MiniMax adjustment is applied last. No supplier API
  price, CNY value, or exchange rate participates in the calculation.

Edge cases in the log:

- Without a streaming usage chunk (after the gateway has requested
  `include_usage` on Chat streams), the row ends with `success_no_usage`.
- Models absent from the snapshot are still forwarded, but finish as
  `success_unpriced`, display no quota cost, and do not enter quota totals.
- Zen free models finish as `success` with `cost_state=free`: tokens are
  recorded, quota cost stays empty, and they do not enter Go quota totals.
- Custom API forwards finish with `cost_state=unknown`, display no quota
  cost, and do not debit any provider quota.
- Pre-snapshot successful rows retain their old value and are marked as a
  legacy estimate; they are never recalculated.
- A manually saved percentage becomes the baseline for that window. Official
  refresh (manual **Refresh quota** or adaptive sync) on a ready Key or managed
  account overwrites the baseline with official OpenCode usage percentages.
  Successful priced costs recorded afterward accumulate until the next manual
  calibration or official refresh. A real inference `429` only writes an
  independent cooldown and affects account selection; it does not rewrite the
  usage baseline, but it does schedule a later official reconciliation.
- An `outcome_unknown` row means the upstream may have completed and charged
  the request while the gateway lost the response; the request is not
  retried and its local cost stays unknown.

The dashboard always pairs a bar with the account's cooldown state — see the
next section.

### True And False Circuit Breakers

- **False circuit breaker (local estimate).** The local estimate is a
  *signal*, not a stop sign. When it reaches the limit, the gateway **keeps
  sending** requests with that account. Local accounting and upstream
  billing/reset boundaries may not match, so a full local bar is a warning,
  not proof that the upstream account is blocked.
- **True circuit breaker (upstream 429).** The gateway stores the upstream
  error, parses the `Resets in …` phrase from the response, writes
  `cooldown_until`, and tries the next available account. The known 5-hour,
  weekly, and monthly limit messages use the reset duration reported by the
  upstream for that cooldown only; they do not rewrite the matching usage
  baseline. During cooldown the matching bar is forced to 100% in the
  dashboard; after cooldown, local priced costs continue from the existing
  baseline until the next manual calibration or official refresh. An
  unrecognized 429 falls back to a five-minute cooldown without changing
  any usage baseline.
- **No account available.** If every enabled account is cooling down, the
  gateway returns `429` with the soonest reset time.
- **Dashboard display.** While a true circuit breaker is active, the matching
  5-hour, weekly, or monthly bar is forced to 100% and marked as an error,
  even when the local estimate is lower. The account becomes eligible
  automatically after `cooldown_until`, or immediately after you reset its
  cooldown in the dashboard.

### Free model policy

Settings expose three OpenCode Zen free-routing modes:

| Mode | Behavior |
| --- | --- |
| **Deny free models** | Reject `*-free` / `big-pickle` and never rewrite Go models onto free |
| **Explicit free only** (default) | Only client-requested free models use `https://opencode.ai/zen`; Go models stay on Go |
| **Prefer mapped free models** | Current maps: `deepseek-v4-flash` → `deepseek-v4-flash-free`, `mimo-v2.5` → `mimo-v2.5-free`; prefer free only when a coarse context estimate fits, otherwise or when free is exhausted (IP-shared 429, no key rotation) fall back to Go |

Free and Go cooldowns are **independent**. Zen Free sends no authentication
headers. Its promo quota is shared per egress IP, so a free `429` cools the
whole free channel and does **not** rotate keys; when a mapped free request can
fall through, prefer mode safely continues on Go. A free `401` or `403` blocks
that route. Multi-account routing still applies on the Go channel. Under
sticky-global routing, Free and Go currently share one preferred account id
across channels.
Successful free-channel rows keep token counts but use `cost_state=free` and do
not enter Go quota totals; prefer-mode fallbacks that land on Go are priced as
usual. Free models are promotional, use a separate quota, and may use request
data to improve models — do not submit confidential content.

## CLI

Download the archive for your platform and extract it as a directory. It
contains the executable, `dist/`, and `LICENSE`. Keep `dist/` beside the
executable so `serve` can serve the dashboard. On Windows the executable is
`ocg-manager-cli.exe`; on Linux you may need `chmod +x ocg-manager-cli` after
extraction.

The CLI data directory defaults to `~/.ocg-mgr-cli` on every platform;
override it with `--data-dir <path>`. The obfuscation secret defaults to
`<data-dir>/.encryption-key`; override it with the named
`--encryption-key <key>` option or the `OCG_MANAGER_ENCRYPTION_KEY`
environment variable.

```text
ocg-manager-cli
├── serve         Start the gateway server
│   --host        Address to listen on (default 127.0.0.1)
│   -p, --port    Gateway port (sets and saves config)
│   --dashboard-dir  Directory containing the built web dashboard
├── key list      List accounts and their enabled state
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

The fastest way to bootstrap a headless gateway:

```bash
./ocg-manager-cli key add main sk-...
./ocg-manager-cli key list
./ocg-manager-cli serve --port 9042
```

`serve --port <port>` writes the new port to SQLite. Later `serve` runs
without `--port` reuse that saved value.

`key ping` reads the obfuscated key, sends a tiny chat completion, and prints
the real upstream status code and a short body excerpt — use it to surface
real `401`/`403`/`429`/`200` from each key without going through the
dashboard.

## Docker

The public headless image can be pulled from GHCR without signing in. It is a
Linux container publishing `linux/amd64` and `linux/arm64`; a plain
`docker pull` selects the matching native variant on either architecture. Each
release also includes a pull-only
`compose.example.yaml`; save it as `compose.yaml` and optionally create a
neighboring `.env`. The example pins its matching release by default, while
`OCG_IMAGE` can override it. Alternatively, run the Compose commands from a
checkout containing `compose.yaml` and `.env.example` (preferably the
matching release tag):

```bash
git clone --branch v1.8.1 --depth 1 https://github.com/klarkxy/opencode-go-mgr.git
cd opencode-go-mgr
cp .env.example .env
# PowerShell: Copy-Item .env.example .env
# Edit .env before exposing the service outside the host.
docker compose pull
docker compose up -d --no-build
docker compose ps
```

### Choosing An Image

- The repository's source-capable `compose.yaml` defaults to
  `ghcr.io/klarkxy/opencode-go-mgr:latest`; the Release
  `compose.example.yaml` defaults to its matching full version.
- For repeatable production deployments, set `OCG_IMAGE` in `.env` to a full
  release tag such as `ghcr.io/klarkxy/opencode-go-mgr:1.8.1`.
- Full-version and `sha-<commit>` tags identify one release and are intended
  not to move; `1.5` and `latest` move forward. Only a digest such as
  `ghcr.io/klarkxy/opencode-go-mgr@sha256:...` is technically immutable.
- To build the current checkout instead, set `OCG_IMAGE=ocg-manager:local`
  and run `docker compose up -d --build`. `NPM_REGISTRY` and
  `CARGO_REGISTRY` are build arguments for that source-build path only; they
  do not change a pulled image.

| Variable | Scope | Meaning |
| --- | --- | --- |
| `OCG_IMAGE` | Compose | Image tag, mirror, local name, or immutable digest. |
| `OCG_BROWSER_IMAGE` | Compose | Optional Chromium/noVNC sidecar image tag, mirror, local name, or digest. |
| `OCG_PORT` | Compose | Host loopback port; the container still listens on `9042`. |
| `OCG_ADMIN_USERNAME` + `OCG_ADMIN_PASSWORD` | First start | Optional administrator bootstrap; both or neither. |
| `OCG_CLIENT_ROOT_URL` | Runtime | Read-only external client root override. |
| `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` / `NO_PROXY` | Runtime | Standard proxy variables used by `Automatic (system / environment)` outbound proxy mode. |
| `OCG_MANAGER_ENCRYPTION_KEY` | Runtime restore | Original explicit obfuscation key, when one was used. |
| `NPM_REGISTRY` + `CARGO_REGISTRY` | Source build | Dependency registries used only by `--build`. |

### Optional Remote Browser

The default gateway deployment does not start the browser sidecar. To use
managed onboarding and website login on a Linux server or Docker host,
reserve at least 2 CPUs, 2 GiB of RAM, and 1 GiB of `/dev/shm`, then run:

```bash
docker compose --profile browser up -d
docker compose ps
```

`OCG_BROWSER_IMAGE` overrides the default
`ghcr.io/klarkxy/opencode-go-mgr-browser:<version>`. The sidecar runs ordinary
Chromium, Xvfb, a lightweight window manager, x11vnc, and noVNC. The dashboard
shows it in a dedicated full browser tab through an authenticated same-origin
WebSocket, including keyboard and pointer input. Use the page's explicit
remote clipboard area to copy or paste a key. A reverse proxy in front of the
dashboard must support WebSocket upgrades.
The sidecar launches Chromium with its basic password store so its persistent
profiles do not depend on a host keyring.

Only one remote Chromium runs per node. Switching accounts first shuts down
the current process cleanly and waits for its profile to flush, then starts
the target account; any older remote page becomes invalid immediately.
Dashboard browser tokens are memory-only, bound to the current administrator
session, and Origin-checked. They expire after 30 minutes idle or four hours
total; reopen the account website to create another session.

The sidecar publishes no host port and never mounts the database. Its control
and noVNC endpoints exist only on the Compose `browser-private` network. This
project-scoped bridge is not Docker `internal`, because Chromium needs outbound
HTTPS access to Google and OpenCode; neither sidecar endpoint is published to
the host. A random control token lives in the shared `ocg-browser-runtime`
runtime volume.
Account cookies and profiles live in `ocg-browser-profiles`; do not back up the
runtime volume, but always stop and back up the two sensitive persistent
volumes, `ocg-data` and `ocg-browser-profiles`, together.

Google may treat a data-center egress IP as high risk, require additional
verification, or reject registration/login. OCG Manager does not bypass that
risk control. Complete Google's checks yourself, or use the desktop build on
a residential connection. Real payment is always an explicit user action on
the official site.

### Administrator Bootstrap

`OCG_ADMIN_USERNAME` and `OCG_ADMIN_PASSWORD` create the administrator **only
when the database has no administrator yet**.

- Both must be set together; setting only one stops startup with an error.
- Once an administrator exists, later environment changes do not reset it.
- When both are omitted, the first visitor creates the administrator in the
  dashboard.
- After the administrator exists, you may remove both variables while keeping
  the volume; the stored account remains. Remove them from the container
  environment with `docker compose up -d --no-build --force-recreate`.

Bootstrap credentials are visible to anyone with Docker daemon access.
Protect `.env`, use a long random password, and do not expose an
uninitialized dashboard publicly.

### Secrets And Addresses

`OCG_MANAGER_ENCRYPTION_KEY` is an advanced restore override. Leave it unset
for normal deployments so the generated `.encryption-key` stays in the data
volume. If the original deployment supplied this variable, the restored
deployment must use the same value; changing or losing it makes saved
credentials unreadable. Treat it like a password.

The optional `OCG_CLIENT_ROOT_URL` is the environment equivalent of the
dashboard's Downstream Access Root. Use it when a reverse proxy is present or
the dashboard and gateway have different externally reachable addresses. A
non-empty value must be an absolute HTTP(S) URL; when present, it overrides
the saved SQLite value, and an invalid value stops startup. It does not
configure the listener, DNS, or reverse proxy. Normally use
`https://ocg.example.com`, not `/dashboard/` or a concrete API endpoint; a
trailing `/v1` is accepted.

### Runtime Behavior

Set `OCG_PORT` in `.env` to change the host port; the container still uses
port `9042`. Open `http://127.0.0.1:<OCG_PORT>/dashboard/` and sign in. Use
`/dashboard/`, not the server root `/`.

- Data and the generated `.encryption-key` obfuscation secret persist in the
  `ocg-data` volume; account browser cookies/profiles persist separately in
  `ocg-browser-profiles`.
- The container process binds `0.0.0.0`, so the dashboard requires
  administrator login even when it is published only on host `127.0.0.1`.
  That host mapping limits reachability; it does not enable the loopback
  login bypass.
- The container's `HEALTHCHECK` opens `127.0.0.1:9042` over TCP every 30
  seconds; there is no `/healthz` route. That TCP check proves only that the
  process is listening — not that the dashboard API, an upstream account, or
  a real model request works.
- Both images run as the unprivileged `ocg` user (UID/GID 10001). The supplied
  Compose services make the root filesystem read-only, mount `/tmp` as tmpfs,
  and drop every Linux capability. The main service also enables
  `no-new-privileges`; the browser service instead uses `seccomp=unconfined`
  so ordinary Chromium can establish its own namespace and renderer seccomp
  sandboxes. The sidecar does not use `--no-sandbox` and has 1 GiB of shared
  memory. `ocg-data` and `ocg-browser-profiles` are the two persistent state
  volumes.
- The startup log contains the Key, so log output and Docker daemon
  access are sensitive. Configure log rotation on the Docker host if its
  defaults are not bounded.

Routine operational checks:

```bash
docker compose config --quiet
docker compose ps
docker compose logs --tail=100 -f ocg-manager
docker compose --profile browser logs --tail=100 -f browser
curl --fail http://127.0.0.1:9042/dashboard/
```

Replace `9042` in the curl command with the configured host `OCG_PORT` when
you changed it.

### Verifying An Image

Both the main and browser images include an SPDX SBOM, BuildKit SLSA
provenance, and a GitHub signed provenance attestation. Inspect and verify a
release with:

```bash
docker buildx imagetools inspect ghcr.io/klarkxy/opencode-go-mgr:1.8.1
docker buildx imagetools inspect ghcr.io/klarkxy/opencode-go-mgr-browser:1.8.1
gh attestation verify \
  oci://ghcr.io/klarkxy/opencode-go-mgr:1.8.1 \
  --repo klarkxy/opencode-go-mgr
gh attestation verify \
  oci://ghcr.io/klarkxy/opencode-go-mgr-browser:1.8.1 \
  --repo klarkxy/opencode-go-mgr
```

The second command requires an authenticated GitHub CLI. Public pulls are
anonymous; if the OCI client still requests registry credentials,
authenticate to `ghcr.io` with a token that can read packages. Provenance
proves how the artifact was produced; it is not a vulnerability scan.

Regenerate the Key if it leaks.

### HTTPS

Point an existing reverse proxy at the loopback port. For example, with
Caddy:

```caddyfile
ocg.example.com {
    reverse_proxy 127.0.0.1:9042
}
```

After signing in, set a non-empty Key before sending API traffic.
Stop the service with `docker compose down`; add `-v` only when you
intentionally want to delete all stored accounts, credentials, keys, cookies,
and browser profiles.

## Data And Security

- **GUI data location.** Windows: `%USERPROFILE%\.ocg-mgr`. macOS / Linux:
  `~/.ocg-mgr`. CLI data defaults to `~/.ocg-mgr-cli` on every platform and
  can be overridden with `--data-dir <path>`.
- **Credential storage.** Account keys and saved login passwords are
  obfuscated before storage; this is not cryptographic protection. The
  macOS / Linux GUI and the CLI also place a `.encryption-key` file inside
  the data directory; **back it up with the database** because losing it
  makes stored credentials unreadable. Obfuscation is not a security
  boundary: anyone with the data directory and its `.encryption-key`, or able
  to run the Windows GUI in the original Windows user/machine context, can
  recover account keys and saved login passwords.
- **Browser profiles.** `browser-profiles/`, or Docker's
  `ocg-browser-profiles`, contains long-lived cookies and official-site login
  state and is not encrypted by OCG Manager at all. Protect, transfer, and
  destroy it with the same care as the database and account keys.
- **No cross-node sync.** Each node manages its own accounts through its own
  dashboard. OCG Manager does not synchronize account credentials between
  nodes.
- **Plain HTTP warning.** A non-loopback `http://` root URL exposes the Key
  and request contents to the network. Use HTTPS or a trusted LAN only.
- **Administrator password.** The single administrator password is stored as
  an Argon2 hash in SQLite. There is no self-service password recovery —
  protect the data directory.
- **Custom API destinations.** Custom base URLs are administrator-trusted.
  Any syntactically valid HTTP or HTTPS origin is allowed, including LAN and
  loopback. URL-embedded credentials are rejected; redirects are never
  followed; dashboard and client credentials are never forwarded. Choose
  destinations you intend to reach from this node.

## Limits

- `/embeddings` is not implemented. Gemini `embedContent` is routed but
  returns a Google-style `501 UNIMPLEMENTED` response.
- Gemini `countTokens` also returns `501`; Gemini CLI is expected to fall
  back to local token estimation. Only `generateContent` and
  `streamGenerateContent` are forwarding actions.
- Non-empty Gemini `safetySettings` return `400` because a different upstream
  protocol cannot preserve their safety semantics. `null` and an empty array
  are accepted because they impose no policy.
- Gemini `cachedContent`, `fileData`, Google Search tools, `urlContext`, Code
  Execution, multimodal function-response parts, function response
  schemas/behavior, `VALIDATED` function calling, candidate counts other than
  one, and response modalities other than `TEXT` return `400`. Use base64
  `inlineData` for PNG, JPEG, GIF, or WebP images.
- Gemini `topK` and `thinkingConfig` are accepted only as cross-protocol
  compatibility hints. A native Chat Completions or Messages upstream may
  ignore them or implement different semantics; exact Gemini-equivalent
  sampling and thinking behavior is not guaranteed.
- Other non-null generation options that cannot be preserved, including
  `seed`, presence/frequency penalties, log-probability controls, and media
  resolution, return `400` instead of being silently discarded.
- Responses is stateless: requests must set `store: false`.
  `previous_response_id`, `conversation`, `store: true`, and
  `background: true` return `400` instead of being silently ignored.
- Responses image URLs and data URLs are supported; `input_image.file_id`
  returns `400` because the gateway has no Files API.
- Structured output and custom-tool grammar formats return `400` when
  cross-protocol conversion cannot preserve their constraints.
- Responses hosted tools such as `web_search`, `web_search_preview`, and
  `tool_search` cannot run on OpenCode-Go. Their declarations are dropped in
  automatic tool mode; explicitly forcing one returns a `400` error.
  Function, custom, and namespace tools are converted normally.
- Streaming token counts are accurate only when upstream emits usage chunks;
  Chat streams request `stream_options.include_usage`. Cost uses the active
  OpenCode Go pricing snapshot. Without usage, logs end as `success_no_usage`.
- Browser onboarding provides only manual page interaction; it does not
  register Google accounts, solve verification challenges, pay, scrape
  pages, or extract keys automatically.
- The installed Windows desktop dashboard can start OCG Manager in the tray
  when the user logs in. Development builds, macOS, Linux, CLI, and Docker do
  not expose that dashboard `auto_start` switch. Docker Compose separately
  uses `restart: unless-stopped`, so its service can restart with the Docker
  daemon.
- The macOS desktop dashboard can hide the Dock icon while retaining the
  menu-bar icon. Windows, Linux, CLI, and Docker do not expose the
  `show_dock_icon` switch.
- Windows / Linux ARM64 and 32-bit x86 builds are not published. RPM, Snap,
  app-store packages, Windows Authenticode signing, and Apple notarization
  are not implemented. That covers desktop installers only; the container
  images (`ghcr.io/klarkxy/opencode-go-mgr` and its `-browser` sidecar)
  publish `linux/amd64` and `linux/arm64`. Updater-enabled installed desktop
  builds can install signed releases from Settings; 1.4.1, development
  builds, the CLI, and Docker use the direct/manual upgrade path.
- Command Code GOAT and SCNet Token Plans can be saved as disabled `pending`
  drafts (`routable=false`). Connection verification returns `501`. They have
  no live inference, usage, pricing, verification runtime, or provider
  guides. Custom API is live under the trusted-administrator boundary in
  [Accounts](#accounts); it is unpriced and has no official usage path.
- Unknown model names return `400` on every supported client format. Clients
  should send published aliases or eligible Custom IDs from authenticated
  `GET /v1/models`. Protected `GET /dashboard/api/application-models` is Go
  aliases ∩ active pricing, not that full client list.

## Troubleshooting

- **The dashboard never opens from the tray.** Another process is bound to
  `127.0.0.1:9042`, or a previous tray app still holds the single-instance
  lock. Quit that process or the previous release tray app and retry. For
  source development only, `scripts/free-dev-port.mjs` clears stale Vite
  processes on port `30001`; it does not release `9042` or the desktop
  single-instance lock.
- **`401 Unauthorized` from the upstream.** The OpenCode-Go account key is
  invalid or revoked. Open the **Accounts** view, replace the key, and try
  again. `key ping <id>` is the fastest way to confirm.
- **Local bar at 100% but requests still succeed.** That is a *false* circuit
  breaker — local accounting only. Continue using the account; the gateway
  will keep forwarding.
- **Local bar at 100% and the gateway returns `429`.** That is a *true*
  circuit breaker. Wait for `cooldown_until`, or reset the cooldown manually
  in the **Accounts** view.
- **Gateway returns `429` with "all accounts cooling down".** Every enabled
  account is in cooldown. Either wait for the soonest reset, or add / enable
  another account.
- **Gateway returns `400` for a model name.** Send a published alias or an
  eligible Custom ID from authenticated `GET /v1/models`. Names with `/`,
  `_`, or whitespace are raw IDs, not kebab aliases. Unknown names and
  overlapping raw IDs fail closed and never call upstream.
- **Saving Command Code GOAT or SCNet does not start routing.** Those Plans
  stay disabled `pending` drafts; verification returns `501`. Use an
  OpenCode Go key, Zen Free, or a verified-then-enabled Custom API card for
  live traffic.
- **Saving Custom API does not start routing.** Create/update stay disabled
  `pending`. Verify with a `2xx` JSON response, then enable explicitly.
  Changing the URL, key, or declared models re-pends verification and
  disables the card.
- **Gemini requests fail with `400` over `safetySettings`.** The gateway
  cannot apply Google's safety thresholds equivalently on a Chat/Messages
  upstream, so it rejects non-empty arrays. Remove the field and retry; do
  not assume the same Google content-safety policy still runs afterwards.
- **Docker first-run registration does not pick up my
  `OCG_ADMIN_PASSWORD`.** The variables are only honored when the database
  has no administrator yet. Use the stored administrator account. Recreate
  `ocg-data` and `ocg-browser-profiles` only for an intentional full reset
  after a verified backup; doing so erases every account, credential,
  setting, cookie, and browser profile.
- **SmartScreen / Gatekeeper warns about the installer or the DMG.** The
  current Windows builds are unsigned and the macOS app is ad-hoc signed. Use
  **Open Anyway** for the first launch; the warning is not a sign of
  tampering.

---

[中文用户指南](USER.zh-CN.md) · [Maintainer guide](MAINTAINER.md) ·
[维护者指南](MAINTAINER.zh-CN.md) · [Docs index](README.md) ·
[Back to README](../README.md)
