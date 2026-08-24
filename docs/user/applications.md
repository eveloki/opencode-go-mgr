[简体中文](applications.zh-CN.md)

# Application Guides And Model Capabilities

## Application Guides

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
`GET /dashboard/api/v3/application-models` response: currently routeable OpenCode
Go aliases intersected with the active OpenCode Go pricing snapshot. Highspeed
variants inherit the base model's pricing row. An empty intersection is `[]`,
not an error. That list is **not** the same as authenticated `GET /v1/models`,
which publishes currently routeable Go aliases, the saved Zen Free catalog,
and eligible Custom declared IDs. `application-models` stays Go-only. Both
endpoints are local reads: no SCNet official table spellings, no unpublished
Command Code GOAT names, and no request-time upstream discovery or account
selection. They publish only currently routable models that have an effective
enabled protocol. Zen Free catalog refresh is an explicit **Providers**
action. An accepted pricing refresh on **Providers** can
change which Go aliases `application-models` returns. The view reloads this
local list whenever you return to it. Model selections and edited snippets are
cached separately per application while the current dashboard page remains
alive; a page reload resets this in-memory state. **Restore defaults** resets
the active application's model selection and snippet drafts.

## Model capabilities

Application snippets use the verified limits below
(`src/views/application-guides.ts`, 2026-08-14). Input is what OCG can
actually carry. The passthrough / conversion matrix is under
[Protocol Conversion](protocol-conversion.md).

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
are saved to SQLite through protected
`GET/PUT /dashboard/api/v3/claude-desktop/models`. Omitted roles
inherit the first configured role, and the three roles cannot all be empty.
Its restore action returns to the mapping loaded or last saved in the current
page.

---

[User guide index](../USER.md) · [简体中文](applications.zh-CN.md) · [Docs index](../README.md)
