import assert from "node:assert/strict";
import test from "node:test";
import type { Account } from "../api/dashboard.ts";
import { accountStatusLabel } from "../domain/account-display.ts";
import { buildPricingOfferingSections } from "../domain/pricing-view.ts";
import type { ProviderCatalogEntry } from "../api/providers.ts";

const catalogEntry = (
  provider_id: string,
  offering_id: string,
): ProviderCatalogEntry => ({
  provider_id,
  offering_id,
  credential_kind: "api_key",
  quota_scope: "key",
  singleton: false,
  display_name: `${provider_id} ${offering_id}`,
  display_family: provider_id,
  creation_availability: "available",
  verification_policy: "not_required",
  verification_runtime_availability: "optional",
  routable: true,
  managed_registration: provider_id === "opencode",
  pricing_availability: "available",
  usage_availability: "available",
  manual_usage_calibration: false,
  quota_unit: "usd",
  model_source: "test",
  auth_schemes: ["bearer"],
  upstream_protocols: ["chat_completions"],
  form_fields: [],
  model_aliases: [],
});

test("catalog entries augment listed flags without inventing sections", () => {
  const sections = buildPricingOfferingSections([
    catalogEntry("opencode", "go"),
    catalogEntry("opencode-zen-free", "anonymous-free"),
    catalogEntry("unknown-provider", "unknown-offering"),
  ]);

  assert.equal(sections.length, 3);
  assert.equal(sections[0]?.label, "OpenCode Go");
  assert.equal(sections[0]?.listed, true);
  assert.equal(sections[1]?.listed, false);
  assert.equal(sections[2]?.label, "Zen Free");
  assert.equal(sections[2]?.listed, true);
});

test("pricing sections treat an empty catalog as no listings", () => {
  const sections = buildPricingOfferingSections([]);
  assert.equal(sections.length, 3);
  assert.ok(sections.every(({ listed }) => !listed));
});

test("GOAT account states are live without a verification phase", () => {
  const goat = (overrides: Partial<Account> = {}): Account => ({
    id: "goat-1",
    name: "GOAT",
    username: "",
    password: "",
    key: "key",
    enabled: false,
    account_type: "key",
    setup_step: "ready",
    provider_id: "command-code",
    offering_id: "goat",
    credential_kind: "api_key",
    quota_scope: "key",
    purchase_date: "",
    expires_on: "",
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
    created_at: "2026-08-21T00:00:00Z",
    updated_at: "2026-08-21T00:00:00Z",
    verification_status: "not_required",
    connection_verified_at: null,
    verification_error: null,
    plan_routable: true,
    model_capabilities: [],
    ...overrides,
  });

  assert.equal(accountStatusLabel(goat()), "已禁用");
  assert.equal(accountStatusLabel(goat({ enabled: true })), "可用");
  // An unroutable catalog still renders the backend-owned draft state.
  assert.equal(accountStatusLabel(goat({ plan_routable: false })), "等待支持");
});
