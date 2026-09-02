import assert from "node:assert/strict";
import test from "node:test";
import type { Account } from "../api/dashboard.ts";
import {
  accountMenuOptions,
  accountStatusLabel,
  accountStatusTagType,
} from "../domain/account-display.ts";

function customAccount(overrides: Partial<Account> = {}): Account {
  return {
    id: "custom-1",
    name: "Custom",
    username: "",
    password: "",
    key: "key",
    enabled: false,
    account_type: "key",
    setup_step: "ready",
    provider_id: "custom",
    offering_id: "api",
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
    verification_status: "pending",
    connection_verified_at: null,
    verification_error: null,
    plan_routable: true,
    model_capabilities: [],
    ...overrides,
  };
}

test("routable Custom account status follows live enablement rather than legacy verification state", () => {
  const pending = customAccount();
  assert.equal(accountStatusLabel(pending), "已禁用");
  assert.equal(accountStatusTagType(pending), "default");

  const failed = customAccount({ verification_status: "failed" });
  assert.equal(accountStatusLabel(failed), "已禁用");
  assert.equal(accountStatusTagType(failed), "default");

  const verified = customAccount({ verification_status: "verified" });
  assert.equal(accountStatusLabel(verified), "已禁用");
  assert.equal(accountStatusTagType(verified), "default");
});

test("Custom cards drop Go-only console/profile actions but keep edit and delete", () => {
  const keys = accountMenuOptions(customAccount(), Date.now()).map(({ key }) => key);
  assert.deepEqual(keys, ["edit", "delete"]);
});
