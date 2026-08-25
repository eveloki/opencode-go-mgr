[简体中文](limits.zh-CN.md)

# Limits

Every gateway draws a line somewhere. This page is that line — a running list of things OCG Manager refuses to do, usually with a `400` instead of a polite lie.

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
  drafts (`routable=false`). Connection verification returns `501`; they have
  no live inference, usage, pricing, verification runtime, or production
  routing and cannot be promoted by probes. They appear on **Providers** as
  non-routable scopes. Custom API is live under the trusted-administrator
  boundary in [Accounts](accounts.md); it is unpriced, has no official usage
  path, and its catalog, protocol, and pricing controls live on **Providers**
  as isolated `CustomEndpoint` scopes.
- Unknown model names return `400` on every supported client format. Clients
  should send published aliases or eligible Custom IDs from authenticated
  `GET /v1/models` that currently have an effective enabled protocol.
  Protected `GET /dashboard/api/v3/application-models` is Go aliases ∩ active
  pricing, not that full client list.

---

[User guide index](../USER.md) · [简体中文](limits.zh-CN.md) · [Docs index](../README.md)
