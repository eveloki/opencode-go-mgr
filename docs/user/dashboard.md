[简体中文](dashboard.zh-CN.md)

# The Dashboard

The dashboard is a single-page Vue 3 application served by the gateway. The
left rail (or the horizontal app menu below 1024px) exposes seven views:
**Dashboard**, **Access Keys**, **Accounts**, **Providers**, **Applications**,
**Logs**, and **Settings**. The top right of the header holds the theme
switcher, the language switcher, and the sign-out button.

The dashboard speaks ten languages: 简体中文, 繁體中文, English, 日本語,
한국어, Español, Français, Deutsch, Português (Brasil), and Русский. The
default is 简体中文. The choice persists in `localStorage` under
`ocg-manager.locale`; when persistence is unavailable (for example in a
private window), the in-memory locale still works for the current session.

## Dashboard V3

The current dashboard SPA talks only to **`/dashboard/api/v3`**. That is the
data path for Connection Center, Access Keys, Accounts, Providers,
Applications, Logs, Settings, and the login/register/logout flow. Writes send
the last seen `expectedRevision` and `processGeneration`. If another tab in
the same running process saves first, the server rejects the stale write with
HTTP 409 (`revisionConflict`). The current SPA does not recognize that exact
server code for automatic conflict recovery, so refresh the affected page and
re-apply the change manually. These tokens are process-local; separate
processes sharing one data directory are not a coordinated CAS domain. The
OpenCode Go pricing snapshot carries its own
`pricingRevision`, independent of those settings tokens.

Plaintext Keys exist only on the Connection Center payload
(`GET /dashboard/api/v3/connection`). The Settings resource never includes
Key values. The browser keeps those secrets in memory only; signing out or a
401 session expiry wipes them immediately.

The seven views stay cached while you switch tabs (`KeepAlive`). Returning to
a view refreshes its server data; the Dashboard also refreshes when you bring
the browser tab back to the foreground. Catalogs, pricing, and provider
directories are never polled automatically. Official usage sync remains a
server-side schedule, not a dashboard poll. After you start a signed desktop
install from Settings, that page may poll install progress until the process
restarts.

A cached older dashboard page that still calls retired `/dashboard/api` REST
(not `/dashboard/api/v3`) receives HTTP 410 with code `dashboardV2Removed` and
guidance to refresh the page, then upgrade if that is not enough. Anonymous
retired REST is 401 before that 410. Two V2 families remain as compatibility
exceptions, not as the current SPA data path: `/dashboard/api/auth/status`,
`/dashboard/api/auth/register`, `/dashboard/api/auth/login`,
`/dashboard/api/auth/logout`, and `/dashboard/api/browser/sessions/{token}/ws`.
The current dashboard uses the V3 auth and browser-WebSocket routes instead.

There is no dashboard **Ping** button. To probe an OpenCode Go key from this
product, use CLI `key ping` or send a real client request. Custom cards still
have **Verify connection**; managed signup still has Key verification.

## Connection Center

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

## Access Keys

The **Access Keys** view is the home for client-facing credentials. Primary
and sub Keys are stored together in `access_keys` (schema v27). Create,
rename, enable, disable, regenerate, and delete go through Dashboard V3; a
successful change bumps the settings revision. Mutation acknowledgements do
not include plaintext — the page reloads Connection Center to show the new
value.

- The **primary key** is always active and cannot be disabled or deleted; you
  rotate it with the reset control. Its id is
  `00000000-0000-0000-0000-000000000001`. It is the credential the application
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

---

[User guide index](../USER.md) · [简体中文](dashboard.zh-CN.md) · [Docs index](../README.md)
