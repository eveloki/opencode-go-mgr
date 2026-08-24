[简体中文](overview.zh-CN.md)

# What OCG Manager Does

OCG Manager keeps provider API keys in a local SQLite database — officially
distributable OpenCode Go keys, plus trusted Custom API destinations — and
exposes a loopback gateway at `http://127.0.0.1:9042/v1`. Each
account card is one **Plan** (provider + offering). Clients send **aliases**
from the local registry or eligible Custom model IDs; live routing is
OpenCode Go, OpenCode Zen Free, and Custom API.
The same gateway also serves the Vue 3 dashboard at `/dashboard/`. The current
dashboard SPA reads and writes JSON at `/dashboard/api/v3`. Every node is
independent: there is no remote sync, no Admin API, and no telemetry.

The gateway does four jobs:

1. Authenticate the client with the **Key** issued by the dashboard.
2. Resolve the requested model against the local Alias registry (and eligible
   Custom declared IDs), then pick a usable account card after capability
   filtering, the adapter ceiling, the saved provider contract, and the
   Chat Completions / Responses / Messages switches.
3. Convert the request to the selected Plan's effective upstream protocol,
   and the response back to the client protocol. Client requests never
   discover or probe.
4. Log the request (`requested_model`, `resolved_alias`, `upstream_model`),
   write usage and any cooldown to SQLite, and surface everything in the
   dashboard.

---

[User guide index](../USER.md) · [简体中文](overview.zh-CN.md) · [Docs index](../README.md)
