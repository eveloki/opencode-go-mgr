import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import type { Account } from "../api/tauri.ts";
import type { ProviderCatalogEntry } from "../api/providers.ts";
import { buildProviderOverviews, providerAccountHealthy, providerPairKey } from "./dashboard-providers.ts";

function account(id: string, provider_id: string, offering_id: string, credential_kind: "api_key" | "none"): Account {
  return {
    id,
    provider_id,
    offering_id,
    credential_kind,
    quota_scope: credential_kind === "none" ? "egress-ip" : "key",
    free_alias_enabled: false,
    name: id,
    username: "",
    password: "",
    key: "",
    enabled: true,
    account_type: "key",
    setup_step: "ready",
    purchase_date: "2026-01-01",
    expires_on: "2026-02-01",
    cooldown_until: null,
    cooldown_generic_until: null,
    cooldown_5h_until: null,
    cooldown_week_until: null,
    cooldown_month_until: null,
    cooldown_free_until: null,
    last_error: null,
    auth_error: null,
    notes: "",
    usage_sync_last_success_at: null,
    usage_sync_next_allowed_at: null,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
  };
}

function catalog(provider_id: string, offering_id: string, credential_kind: "api_key" | "none"): ProviderCatalogEntry {
  return {
    provider_id,
    offering_id,
    credential_kind,
    quota_scope: credential_kind === "none" ? "egress-ip" : "key",
    singleton: credential_kind === "none",
    pricing_availability: provider_id === "command-code" ? "unconfigured" : "available",
    usage_availability: provider_id === "command-code" ? "unconfigured" : "available",
  };
}

test("provider dashboard keeps Zen credentialless, GOAT fail-closed, and unknown cost distinct from zero", () => {
  const entries = [
    catalog("opencode", "go", "api_key"),
    catalog("command-code", "goat", "api_key"),
    catalog("opencode-zen-free", "anonymous-free", "none"),
  ];
  const rows = buildProviderOverviews([
    account("go", "opencode", "go", "api_key"),
    account("goat", "command-code", "goat", "api_key"),
    account("zen", "opencode-zen-free", "anonymous-free", "none"),
  ], entries, { [providerPairKey("opencode", "go")]: 2.5 }, Date.now());

  assert.deepEqual(rows.map(({ healthy }) => healthy), [1, 0, 1]);
  assert.equal(rows[0]?.cost, 2.5);
  assert.equal(rows[0]?.cost_state, "known");
  assert.equal(rows[1]?.cost, null);
  assert.equal(rows[1]?.cost_state, "unknown");
  assert.equal(rows[2]?.cost, 0);
  assert.equal(rows[2]?.cost_state, "free");
});

test("provider health honors provider-specific cooldowns and ignores expired or foreign ones", () => {
  const now = Date.parse("2026-01-15T12:00:00Z");
  const future = "2026-01-15T13:00:00Z";
  const past = "2026-01-15T11:00:00Z";

  const go = (overrides: Partial<Account>) => ({
    ...account("go", "opencode", "go", "api_key"),
    ...overrides,
  });
  const zen = (overrides: Partial<Account>) => ({
    ...account("zen", "opencode-zen-free", "anonymous-free", "none"),
    ...overrides,
  });

  // Active Go cooldowns (generic/legacy and per-window) block health.
  for (const field of [
    "cooldown_until",
    "cooldown_generic_until",
    "cooldown_5h_until",
    "cooldown_week_until",
    "cooldown_month_until",
  ] as const) {
    assert.equal(providerAccountHealthy(go({ [field]: future }), now), false, field);
    assert.equal(providerAccountHealthy(go({ [field]: past }), now), true, `${field} expired`);
  }
  // Go never honors the Zen free lane cooldown.
  assert.equal(providerAccountHealthy(go({ cooldown_free_until: future }), now), true);

  // Zen free honors generic/legacy plus its own free lane cooldown.
  for (const field of ["cooldown_until", "cooldown_generic_until", "cooldown_free_until"] as const) {
    assert.equal(providerAccountHealthy(zen({ [field]: future }), now), false, field);
    assert.equal(providerAccountHealthy(zen({ [field]: past }), now), true, `${field} expired`);
  }
  // Zen free ignores the Go per-window cooldowns.
  assert.equal(providerAccountHealthy(zen({ cooldown_5h_until: future }), now), true);

  // GOAT stays fail-closed even with valid credentials and no cooldown.
  assert.equal(providerAccountHealthy(
    account("goat", "command-code", "goat", "api_key"),
    now,
  ), false);
  // Malformed cooldown timestamps never block health.
  assert.equal(providerAccountHealthy(go({ cooldown_until: "not-a-date" }), now), true);
});

test("dashboard loads provider-filtered remote summaries and skips legacy usage for non-Go cards", () => {
  const source = readFileSync(new URL("./Dashboard.vue", import.meta.url), "utf8");
  assert.match(source, /providerApi\.getProviderCatalog\(\)/);
  assert.match(source, /tauriApi\.getForwardLogs\(\{/);
  assert.match(source, /provider_id: go\.provider_id/);
  assert.match(source, /account\.provider_id === "opencode"/);
  assert.match(source, /account\.offering_id === "go"/);
  assert.match(source, /provider\.cost_state === "unknown"[^]*?t\("未知"\)/);
  assert.match(source, /供应商尚未配置/);
  // Cost copy says cumulative, not "current log range".
  assert.match(source, /t\("账号健康与累计成本"\)/);
  assert.match(source, /t\("累计成本"\)/);
  assert.doesNotMatch(source, /当前日志范围成本/);
});
