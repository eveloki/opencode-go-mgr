[简体中文](conventions.zh-CN.md)

# Coding Conventions

- **Ponytail principle.** Prefer deleting code over adding code; reuse
  existing helpers before adding new abstractions. The codebase favors flat
  call sites over speculative indirection — but do not omit required CAS,
  tombstones, or fail-closed checks.
- **Keep the crate DAG.** Domain and gateway stay I/O-free. Facades reexport
  item-by-item. Adapters return `AttemptSpec`. `forward_once` is one
  upstream call. Dashboard V3 does not import `gateway`.
- **No Tauri `invoke()` paths.** The Vue data path is HTTP
  `/dashboard/api/v3`. Do not register `generate_handler`.
- **Do not revive protected V2 REST.** New JSON is V3. The 410 tombstone
  stays in front of retired `/dashboard/api/...` paths.
- **Do not weaken security boundaries.** Gateway authentication, key
  obfuscation, URL validation, cooldown writes, SSE pass-through, and the
  ConnectionInfo secret boundary are not simplification candidates.
- **Do not add remote sync.** Each node is managed through its own dashboard.
- **Capability-gate `auto_start` and `show_dock_icon`.** Only the Windows
  release/installed Tauri process injects the registry sync hook; Dock is
  macOS Tauri only.
- **Local Alias lists stay local.** Authenticated `GET /v1/models` and
  dashboard `application-models` must not grow request-time upstream
  discovery. The explicit Zen Free refresh on Providers is the only
  directory-fetch exception and is restricted to the fixed official
  endpoint. Do not equate the two lists; do not invent a `requested_alias`
  log field.
- **Don't re-invent `cargo test` ergonomics.** The CLI and core use
  `parking_lot::Mutex`, which is not re-entrant. When a function needs to
  call another lock holder, `drop` the guard first.
- **Match the surrounding style.** When you change code in a file, the new
  code should look like the old code: same comment density, naming, and
  idiom.
---

[Maintainer guide index](../MAINTAINER.md) · [简体中文](conventions.zh-CN.md) · [Docs index](../README.md)
