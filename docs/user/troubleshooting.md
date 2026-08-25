[简体中文](troubleshooting.zh-CN.md)

# Troubleshooting

Troubleshooting OCG Manager usually starts with discovering that something else
is already squatting on `127.0.0.1:9042`. The entries below cover stale SPAs,
conflicting writes, accounts that are cooling down, and Plans that look ready
but are still `pending` drafts — the gateway stays pessimistic so you do not
get billed for a bad guess.

- **The dashboard never opens from the tray.** Another process is bound to
  `127.0.0.1:9042`, or a previous tray app still holds the single-instance
  lock. Quit that process or the previous release tray app and retry. For
  source development only, `scripts/free-dev-port.mjs` clears stale Vite
  processes on port `30001`; it does not release `9042` or the desktop
  single-instance lock.
- **`401 Unauthorized` from the upstream.** OpenCode Go and Zen Free return it
  to the client without rotating accounts; OpenCode Go also uses 401 for
  `ModelError` when the model is not on that product. Custom API `401` rotates
  to the next eligible card and records `auth_error`. To check an OpenCode Go
  key directly, use CLI `key ping <id>` or send a real client request.
  Managed-account Key verification and Custom **Verify connection** still
  record `auth_error` on 401 in those flows.
- **The dashboard says the page version does not match the service.** A cached
  older SPA hit retired `/dashboard/api` REST (not `/dashboard/api/v3`) and
  received HTTP 410. Refresh the page; if that is not enough, install the
  matching desktop, CLI, or Docker build.
- **A dashboard save failed with a conflict / 409.** Another tab in the same
  running process wrote first. The current SPA does not automatically recover
  from the server's `revisionConflict` code; refresh the affected page, then
  re-apply the change.
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
  cannot map Google's safety thresholds to a Chat/Messages upstream, so it
  rejects non-empty arrays. Remove the field and retry; do not assume the
  same Google content-safety policy still applies.
- **Docker first-run registration does not pick up my
  `OCG_ADMIN_PASSWORD`.** These variables are only honored when the database
  has no administrator yet; use the stored administrator account. Recreate
  `ocg-data` and `ocg-browser-profiles` only for an intentional full reset
  after a verified backup — doing so erases every account, credential,
  setting, cookie, and browser profile.
- **SmartScreen / Gatekeeper warns about the installer or the DMG.** The
  current Windows builds are unsigned and the macOS app is ad-hoc signed. Use
  **Open Anyway** for the first launch; the warning is not a sign of
  tampering.

---

---

[User guide index](../USER.md) · [简体中文](troubleshooting.zh-CN.md) · [Docs index](../README.md)
