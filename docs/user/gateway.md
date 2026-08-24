[简体中文](gateway.zh-CN.md)

# Gateway Behavior

## Endpoints

The gateway is served at `http://<bind>:<port>` and exposes:

| Method | Path | Purpose |
| --- | --- | --- |
| `POST` | `/v1/chat/completions` | OpenAI Chat Completions |
| `POST` | `/v1/responses` | OpenAI Responses |
| `POST` | `/v1/messages` | Anthropic Messages |
| `GET`  | `/v1/models` | Authenticated local list: published Go aliases, the last saved Zen Free catalog, and eligible Custom IDs that currently have an effective enabled protocol |
| `POST` | `/v1beta/models/{model}:generateContent` | Gemini non-stream generation (`/v1/models/...` is also accepted) |
| `POST` | `/v1beta/models/{model}:streamGenerateContent` | Gemini SSE generation (`/v1/models/...` is also accepted) |
| `POST` | `/v1beta/models/{model}:countTokens` | Returns `501`; Gemini CLI can fall back to local estimation |
| `POST` | `/v1beta/models/{model}:embedContent` | Returns `501`; embeddings are not supported |
| `GET`  | `/claude-desktop/v1/models` | Claude Desktop alias model list |
| `POST` | `/claude-desktop/v1/messages` | Claude Desktop Messages with alias rewriting |
| `GET`  | `/dashboard/` | Vue 3 dashboard (HTML) |
| `*`    | `/dashboard/api/v3/...` | Current dashboard JSON API |
| `*`    | `/dashboard/api/...` | Retired V2 REST (authenticated 410 `dashboardV2Removed`), except the labeled V2 auth and browser-WebSocket compatibility routes |

The default bind is `127.0.0.1:9042`. The CLI can override the host with
`serve --host 0.0.0.0` and the port with `serve --port <port>`. The desktop
app also binds loopback and uses a Tauri single-instance lock to prevent two
tray apps from competing for the port. There is no HTTP health endpoint;
Docker checks container-internal TCP port `9042`.

## Authentication

Gateway API endpoints require the **Key** in one of three header
forms: `Authorization: Bearer <key>`, `x-api-key: <key>`, or
`x-goog-api-key: <key>`. Before forwarding, the gateway strips the client
auth header and injects the selected account credential instead. OpenCode Go
uses `x-api-key` for Messages upstreams and `Authorization: Bearer` for Chat
Completions / Responses. Custom API constructs only the configured Bearer or
`x-api-key` header and never forwards dashboard or client credentials.

Dashboard authentication depends on the listener bind. The current SPA uses
`/dashboard/api/v3/auth/status`, `/dashboard/api/v3/auth/register`,
`/dashboard/api/v3/auth/login`, and `/dashboard/api/v3/auth/logout`. Register,
login, and logout require the same `expectedRevision` / `processGeneration`
tokens as other V3 writes. The matching `/dashboard/api/auth/...` routes are
preserved only as a labeled V2 compatibility exception for cached older
pages; they are not the current SPA data path.

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

## Aliases

Clients should send **aliases**: stable lowercase kebab-case names from the
local registry. Existing OpenCode Go model IDs are the preferred aliases;
case-folded spellings such as `GLM-5.2` are accepted.

Authenticated `GET /v1/models` lists currently routeable published aliases
(OpenCode Go and Zen Free) that have an effective enabled protocol, in
deterministic registry order, then appends eligible Custom capability IDs
that do not match those aliases (`owned_by` is `custom`) and likewise have an
effective enabled protocol. It does not make an upstream request: Zen
discovery happens only when an administrator refreshes the catalog on
**Providers**, and this endpoint reads that saved snapshot. It does not write
a forward log. Published Go and Zen Free aliases do not depend on whether any
Go account exists. Eligible Custom IDs come from enabled + verified + ready
Custom accounts that have a key. Dynamic or probe-confirmed models do not
gain a new stable alias automatically.

Protected `GET /dashboard/api/v3/application-models` is a different local list:
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

---

[User guide index](../USER.md) · [简体中文](gateway.zh-CN.md) · [Docs index](../README.md)
