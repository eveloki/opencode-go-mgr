[简体中文](architecture.zh-CN.md)

# Architecture Diagrams

These are text maps of a single local node, current as of HEAD. Live routes: OpenCode Go, Zen Free, and Custom API. Command Code GOAT and SCNet Token Plans are still disabled drafts — routing traffic there will only earn you a `501`. Each diagram points to the chapter that owns the details; when a picture and a chapter disagree, trust the chapter and the code.

## Contents

- [One node, one port](#one-node-one-port)
- [A client request](#a-client-request)
- [Plans: live vs draft](#plans-live-vs-draft)
- [Dashboard, Key, and account cards](#dashboard-key-and-account-cards)
- [Two local model lists](#two-local-model-lists)
- [Protocol conversion](#protocol-conversion)
- [Where to read next](#where-to-read-next)

## One node, one port

Desktop, CLI, and Docker are just three ways to host the same `ocg-core` process. The default bind is `127.0.0.1:9042`. The tray app opens the dashboard in your system browser; it does not remote-control the UI through Tauri `invoke`. There is no remote sync, Admin API, or telemetry.

```text
   Desktop tray          CLI `serve`           Docker
   (ocg-manager)      (ocg-manager-cli)   (ghcr.io/.../opencode-go-mgr)
           \                  |                    /
            \                 |                   /
             +----------------+------------------+
             |            ocg-core               |
             |         127.0.0.1:9042            |
             +----------------+------------------+
                    /                    \
                   /                      \
        GET /dashboard/              inference
        Vue 3 SPA                    /v1/chat/completions
        (system browser)             /v1/responses
                                     /v1/messages
                                     /v1/models
                                     Gemini generateContent
                                     /claude-desktop/v1/...
                   \                      /
                    \                    /
             +----------------+------------------+
             |         SQLite schema v27         |
             |  GUI  ~/.ocg-mgr                  |
             |  CLI  ~/.ocg-mgr-cli              |
             +-----------------------------------+
```

Install, first client, CLI, and Docker: [Install](install.md),
[First client](first-client.md), [CLI](cli.md), [Docker](docker.md).

## A client request

The dashboard **Key** authenticates the client to this node. The selected account's credential is what this node sends upstream — Zen Free has none. Quota bars are warnings, not gates; only an upstream `429` cools a card.

```text
  AI client                         this node                         Plan
  ---------                         ---------                         ----
      |                                  |                              |
      |  Key + alias / Custom ID         |                              |
      |--------------------------------->|                              |
      |                                  | 1. authenticate Key          |
      |                                  |    Bearer / x-api-key /      |
      |                                  |    x-goog-api-key            |
      |                                  | 2. resolve alias             |
      |                                  | 3. pick a usable card        |
      |                                  | 4. passthrough or convert    |
      |                                  |----------------------------->|
      |                                  |                              |
      |                                  |<-----------------------------|
      |                                  | 5. convert response          |
      |                                  | 6. log requested_model,      |
      |                                  |    resolved_alias,           |
      |                                  |    upstream_model            |
      |<---------------------------------|                              |
```

`GET /v1/models` is a local list and does not call upstream. Unknown names
return `400` on Chat, Responses, Messages, and Gemini generate / stream.
Overlapping raw IDs return `400` `ambiguous_model_id` and never call
upstream.

Auth, aliases, selection, and breakers: [Gateway](gateway.md),
[Routing](routing.md).

## Plans: live vs draft

Every account card is one Plan (`provider_id` + `offering_id`). Live Plans carry production traffic; draft Plans sit as disabled `pending` cards — `verify` returns `501` and enabling them is not an option.

```text
  LIVE (routable)                         DRAFT (not routed)
  -----------------                       ------------------
  OpenCode Go                             Command Code GOAT
    official key, /zen/go                 verify 501
  Zen Free                                SCNet Token Plans
    no upstream key                       (basic / standard / premium)
    catalog refresh on Providers          verify 501
  Custom API
    trusted-admin HTTP/HTTPS origin


  Custom API lifecycle

    save / update  ->  disabled pending
           |
           v
    verify first declared model
    (one minimal non-stream request;
     2xx JSON object only)
           |
           v
    explicit enable  ->  eligible for routing

  Key, base URL, or declared capability change
  re-pends verification and disables the card.
  Protocol and auth scheme are fixed at create.
```

Zen Free has only an enable switch; turn the card off if you do not want it. Catalog refresh is a Providers action, not an account-card action. For accounts and providers, see [Accounts](accounts.md) and [Providers](providers.md).

## Dashboard, Key, and account cards

The sidebar has seven views. `browser` is a hosted-session overlay, not a hidden eighth. The SPA reads and writes `/dashboard/api/v3`. Loopback listeners skip dashboard login unless forwarding headers are present; clients still need the Key for `/v1`.

```text
  Dashboard -> Access Keys -> Accounts -> Providers
      ^                                      |
      |                                      v
  Settings <- Logs <- Applications <---------+

  Connection Center (Dashboard, first screen)
    copy API root / Key / rotate the current Key
  Access Keys
    create, rename, enable, delete, reset
    Primary Key cannot be disabled or deleted


  two secrets, two directions

    AI client --Key--> this node --account credential--> Plan

    Key            access_keys (schema v27)
                   Primary + optional sub keys (64 active cap)
    Account cred   Go key, Custom key, or Zen Free (none)
```

The only V3 payload that returns Key plaintext is `GET /dashboard/api/v3/connection`. For views, CAS, and where data lives, see [Dashboard](dashboard.md) and [Data and security](data-security.md).

## Two local model lists

Neither GET makes an upstream discovery call. The one exception is Zen Free catalog refresh, which is an explicit Providers action.

```text
  GET /v1/models                         (clients; Key required)
    published Go aliases
      union last saved Zen Free aliases
      union eligible Custom declared IDs
    eligible Custom = enabled + verified + ready + non-empty key
    Custom IDs must not steal published Go/Zen aliases

  GET /dashboard/api/v3/application-models   (dashboard session)
    Go routeable aliases intersect current Go pricing snapshot
    highspeed variants inherit the base price row
    empty intersection is []
    no Custom IDs

  GET /claude-desktop/v1/models
    only the three role aliases (sonnet / opus / haiku)
```

Applications picker vs client list: [Applications](applications.md),
[Gateway](gateway.md).

## Protocol conversion

The request path never probes a protocol — that would double-bill. Gemini is a client format only; no traffic reaches Google.

```text
  client wire
    Chat Completions / Responses / Messages / Gemini generateContent
           |
           v
  alias resolved and a card selected
           |
           +-- client protocol in supported and enabled? -- yes --> passthrough
           |                                                      |
           no                                                     |
           v                                                      |
  convert request body to the Plan's preferred / declared         |
  upstream protocol                                               |
           |                                                      |
           +--------------------------+---------------------------+
                                      v
                               upstream Plan
                                      |
                                      v
                          convert response (or SSE) back
```

Preferred / supported table and conversion limits:
[Protocol conversion](protocol-conversion.md).

## Where to read next

| If you want… | Open |
| --- | --- |
| Install and a first curl | [Install](install.md), [First client](first-client.md) |
| Quota bars vs real cooldowns | [Routing](routing.md) |
| Proxy modes | [Logs and settings](logs-settings.md) |
| Crate DAG, `host_router`, executor | [Maintainer architecture](../maintainer/architecture.md) |

---

[User guide index](../USER.md) · [简体中文](architecture.zh-CN.md) · [Docs index](../README.md)
