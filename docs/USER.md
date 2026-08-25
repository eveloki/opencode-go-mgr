[简体中文](USER.zh-CN.md)

# User Guide

This guide is for people running OCG Manager as a desktop app, a headless gateway, or a Docker service. It walks through installation, daily use of the dashboard and the gateway, and troubleshooting in the order you will meet them.

## Chapters

- [What OCG Manager Does](user/overview.md) — Product positioning and the four jobs the gateway performs.
- [Architecture Diagrams](user/architecture.md) — Text maps of one node, a client request, Plans, and the dashboard.
- [Install And First Run](user/install.md) — Windows, macOS, and Linux desktop installers and first launch.
- [Connect Your First Client](user/first-client.md) — Copy the Key and base URL, then verify a request from a client.
- [Upgrade, Backup, Restore, And Uninstall](user/upgrade-backup.md) — Updater channel, manual upgrade, backup, restore, and uninstall.
- [The Dashboard](user/dashboard.md) — The seven views, i18n, and Connection Center.
- [Application Guides And Model Capabilities](user/applications.md) — 16 client tutorials and the model capability table.
- [Accounts](user/accounts.md) — Plans, credentials, ordering, quota behavior, and managed onboarding.
- [Providers](user/providers.md) — Catalog, provider contracts, protocol switches, and probes.
- [Logs And Settings](user/logs-settings.md) — Request logs, settings, proxy modes, and theme.
- [Gateway Behavior](user/gateway.md) — Endpoints, authentication, aliases, Zen Free, and circuit breakers.
- [Protocol Conversion](user/protocol-conversion.md) — Preferred/supported protocols, passthrough, and conversion limits.
- [Routing, Cost, And Failover](user/routing.md) — Selection order, sticky/round-robin, cost accounting, and failover.
- [CLI](user/cli.md) — Headless CLI archive, data directory, and `serve` / `key` / `status`.
- [Docker](user/docker.md) — GHCR image, Compose setup, browser sidecar, and source builds.
- [Data And Security](user/data-security.md) — Data locations, credential storage, and encryption boundaries.
- [Limits](user/limits.md) — Unimplemented endpoints, Gemini conversion ceiling, and known gaps.
- [Troubleshooting](user/troubleshooting.md) — Common first-run, auth, routing, and log problems.

## Reading paths

- **New user** — `overview` → `architecture` → `install` → `first-client` → `accounts` → `providers` → `gateway` → `applications` → `troubleshooting`.
- **Docker / CLI operator** — `overview` → `architecture` → `docker` → `cli` → `accounts` → `providers` → `routing` → `logs-settings` → `troubleshooting`.

---

[Docs index](README.md) · [简体中文](USER.zh-CN.md)
