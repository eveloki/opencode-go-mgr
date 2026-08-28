[简体中文](gateway.zh-CN.md)

# Gateway Behavior

OCG Manager exposes one HTTP surface on `127.0.0.1:9042` that speaks five client protocols and routes requests to whichever eligible OpenCode Go, Zen Free, Command Code GOAT, MiniMax CN, Kimi Code CN, or Custom API account wins selection — so every client can keep believing every upstream speaks the same dialect.

## Endpoints

The gateway listens on `http://<bind>:<port>` and exposes these endpoints:

| Method | Path | Purpose |
| --- | --- | --- |
| `POST` | `/v1/chat/completions` | OpenAI Chat Completions |
| `POST` | `/v1/responses` | OpenAI Responses |
| `POST` | `/v1/messages` | Anthropic Messages |
| `GET`  | `/v1/models` | Authenticated local list: routeable aliases authorized by the original OpenCode Go table, plus eligible Custom IDs that currently have an effective enabled protocol |
| `POST` | `/v1beta/models/{model}:generateContent` | Gemini non-stream generation (`/v1/models/...` is also accepted) |
| `POST` | `/v1beta/models/{model}:streamGenerateContent` | Gemini SSE generation (`/v1/models/...` is also accepted) |
| `POST` | `/v1beta/models/{model}:countTokens` | Returns `501`; Gemini CLI can fall back to local estimation |
| `POST` | `/v1beta/models/{model}:embedContent` | Returns `501`; embeddings are not supported |
| `GET`  | `/claude-desktop/v1/models` | Claude Desktop alias model list |
| `POST` | `/claude-desktop/v1/messages` | Claude Desktop Messages with alias rewriting |
| `GET`  | `/dashboard/` | Vue 3 dashboard (HTML) |
| `*`    | `/dashboard/api/v3/...` | Current dashboard JSON API |
| `*`    | `/dashboard/api/...` | Retired V2 REST (authenticated 410 `dashboardV2Removed`), except the labeled V2 auth and browser-WebSocket compatibility routes |

Default bind is `127.0.0.1:9042`. Override with `serve --host 0.0.0.0` and `serve --port <port>` in the CLI. The desktop app also binds loopback and uses Tauri's single-instance lock so two tray icons do not fight over the port. There is no HTTP health endpoint; Docker only checks TCP `9042` from inside the container.

## Authentication

Gateway API endpoints need the **Key** in one of three header forms: `Authorization: Bearer <key>`, `x-api-key: <key>`, or `x-goog-api-key: <key>`. The gateway strips the client auth header before forwarding and injects the selected account's credential instead. OpenCode Go sends `x-api-key` to Messages upstreams and `Authorization: Bearer` to Chat Completions / Responses. Custom API derives the one upstream header from its selected protocol: Messages uses `x-api-key`, while Chat Completions and Responses use Bearer. It never sends both or forwards dashboard/client credentials.

Dashboard auth depends on the listener bind. The current SPA uses `/dashboard/api/v3/auth/status`, `/dashboard/api/v3/auth/register`, `/dashboard/api/v3/auth/login`, and `/dashboard/api/v3/auth/logout`. Register, login, and logout need the same `expectedRevision` / `processGeneration` tokens as other V3 writes. The matching `/dashboard/api/auth/...` routes are preserved only as a labeled V2 compatibility exception for cached older pages; they are not the current SPA data path.

- **Loopback binds (the default).** Requests that come straight to the loopback address skip dashboard login unless they carry `Forwarded`, `x-forwarded-for`, `x-forwarded-proto`, or `x-real-ip`; any of those headers requires login. The client still needs the **Key** to reach the upstream endpoints. This is what the desktop app and the default CLI use.
- **Non-loopback binds.** A single administrator account, stored as an Argon2 password hash in SQLite, governs the dashboard. Sign-in returns an HttpOnly session cookie. Standard reverse-proxy forwarding headers on a non-loopback bind still require the cookie. In Docker, the first administrator can be bootstrapped with `OCG_ADMIN_USERNAME` and `OCG_ADMIN_PASSWORD`; otherwise the first registration wins.

## Aliases

Clients send **aliases**: stable lowercase kebab-case names from the local registry. The original static OpenCode Go protocol table is the sole built-in Alias authority; case-folded spellings such as `GLM-5.2` are accepted.

Authenticated `GET /v1/models` returns the currently routeable aliases authorized by that static Go table, in registry order, then appends eligible Custom capability IDs that do not collide with those aliases (`owned_by` is `custom`) and also have an effective enabled protocol. It never calls upstream: explicit catalog refreshes only update saved Provider mappings and contracts. It does not write a forward log. A saved Zen, Command, MiniMax, or Kimi row may join an already-authorized Alias, but no refreshed or probe-confirmed built-in model can create a new Alias. Eligible Custom IDs come from enabled + ready Custom accounts that have a key (verification is optional).

Protected `GET /dashboard/api/v3/application-models` is a different local list: currently routeable OpenCode Go aliases intersected with the active OpenCode Go pricing snapshot. Highspeed variants inherit the base row. An empty intersection returns `[]`. It never includes Custom IDs, never selects an account, and never calls upstream.

`/v1/models` may publish Zen, Command Code, MiniMax, or Kimi mappings through the same canonical Alias authorized by the static Go table; it publishes an Alias only while at least one Provider mapping has an enabled protocol. A built-in catalog ID with no static Alias match remains available only as its exact raw ID and is not advertised as a new Alias. Eligible Custom declared IDs may appear even when they contain `/`; they are not folded into kebab aliases. `application-models` remains the narrower Go-and-pricing list.

A raw upstream ID with exactly one registry mapping is pinned to that mapping — no cross-Plan fallback or Zen prefer overlay — and routability is checked afterward. Built-in raw IDs are exact and case-sensitive. Names containing `/`, `_`, or whitespace are also never folded into kebab aliases (`glm/5.2` is not `glm-5.2`). Custom capability IDs keep their existing case-folded matching behavior. An exact raw ID that matches more than one mapping, including an eligible Custom capability and another Plan, returns `400` with code `ambiguous_model_id` and does not call upstream. Unknown names — neither an authorized alias, an exact saved built-in raw ID, nor an eligible Custom ID — return `400` on every supported client format: Chat Completions, Responses, Messages, and Gemini `generateContent` / `streamGenerateContent`. The canonical kebab alias `deepseek-v4-flash` can select among enabled Go, Zen, and Command Code mappings only because it exists in the static Go table; the unique raw ID `deepseek/deepseek-v4-flash` pins only to Command Code. A Zen `foo-free` row gains the suffix-stripped Alias `foo` only when `foo` is statically authorized; otherwise only the exact `foo-free` raw pin is available.

Forward logs separate the request identity from the upstream identity. There is no `requested_alias` field:

- `requested_model` — the alias or model name the client sent
- `resolved_alias` — the canonical kebab alias when one exists
- `upstream_model` — the Plan's raw upstream ID

plus `provider_id` and `offering_id`. Native cost fields are optional.

Claude Desktop keeps its own three-role alias layer (`claude-sonnet-4-6`, `claude-opus-4-6`, and `claude-haiku-4-5-20251001`). These are rewritten to the mapping saved in **Applications** before Alias resolution. `GET /claude-desktop/v1/models` still advertises only those three role aliases, not the Plan model union.

---

[User guide index](../USER.md) · [简体中文](gateway.zh-CN.md) · [Docs index](../README.md)
