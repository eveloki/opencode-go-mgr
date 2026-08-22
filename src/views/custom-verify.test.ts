import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import type { Account } from "../api/tauri.ts";
import {
  accountMenuOptions,
  accountStatusLabel,
  accountStatusTagType,
} from "./account-display.ts";

const accounts = readFileSync(new URL("./Accounts.vue", import.meta.url), "utf8");
const card = readFileSync(new URL("../components/AccountCard.vue", import.meta.url), "utf8");
const form = readFileSync(new URL("../components/AccountFormModal.vue", import.meta.url), "utf8");
const usage = readFileSync(new URL("./useAccountUsage.ts", import.meta.url), "utf8");

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
    acknowledgements: [],
    ...overrides,
  };
}

test("routable Custom accounts surface verification state instead of a bare disabled label", () => {
  const pending = customAccount();
  assert.equal(accountStatusLabel(pending), "待验证");
  assert.equal(accountStatusTagType(pending), "warning");

  const failed = customAccount({ verification_status: "failed" });
  assert.equal(accountStatusLabel(failed), "验证失败");
  assert.equal(accountStatusTagType(failed), "error");

  const verified = customAccount({ verification_status: "verified" });
  assert.equal(accountStatusLabel(verified), "已禁用");
  assert.equal(accountStatusTagType(verified), "default");
});

test("Custom cards drop Go-only console/profile actions but keep edit and delete", () => {
  const keys = accountMenuOptions(customAccount(), Date.now()).map(({ key }) => key);
  assert.deepEqual(keys, ["edit", "delete"]);
});

test("the verify flow is revision-guarded and never claims enablement", () => {
  const body = accounts.slice(
    accounts.indexOf("async function verifyCustomAccount"),
    accounts.indexOf("async function saveCustomAccountEdit"),
  );
  assert.match(body, /runWithFreshSettingsRevision\(\(revision\) => \(\s*tauriApi\.verifyAccountConnection\(id, revision\)/);
  assert.match(body, /message\.success\(t\("连接验证成功，账号保持禁用，可手动启用。"\)\)/);
  assert.doesNotMatch(body, /toggleAccount|已启用/);
  // Failures refresh server state so the failed verification status renders.
  assert.match(body, /catch[\s\S]*?refreshAccountState\(id\)/);
});

test("Custom edits validate before dispatching only their changed sections", () => {
  const start = accounts.indexOf("async function saveCustomAccountEdit");
  const body = accounts.slice(start, accounts.indexOf("async function toggleAccount"));
  assert.match(body, /executeCustomAccountEdit\(editing, payload/);
  // Success (and closing the modal) still happen only after the executor returns.
  const executeAt = body.indexOf("executeCustomAccountEdit");
  const successAt = body.indexOf('message.success(t("账号已更新"))');
  const closeAt = body.indexOf("showModal.value = false");
  assert.ok(executeAt < successAt && successAt < closeAt);
  const catchBody = body.slice(body.indexOf("catch"));
  assert.doesNotMatch(catchBody, /showModal\.value = false/);
  assert.match(catchBody, /refreshAccountState\(editing\.id\)/);
});

test("no Go usage or official refresh is ever requested for Custom accounts", () => {
  assert.match(accounts, /if \(accountHasUsageDisplay\(created\) && accountIsReady\(created\)\)/);
  assert.match(accounts, /function accountHasUsageDisplay[\s\S]*isCommandCodeGoatAccount[\s\S]*provider_id === "opencode"[\s\S]*offering_id === "go"/);
  assert.doesNotMatch(accounts, /isCustomApiAccount\(created\)/);
  assert.match(accounts, /accountIsReady\(account\) && accountHasUsageDisplay\(account\)/);
  assert.match(usage, /async function retryQuotaLimits[\s\S]*?account\.provider_id === "opencode"[\s\S]*?account\.offering_id === "go"/);
});

test("the card exposes verify for pending/failed Custom accounts and gates the enable switch", () => {
  assert.match(card, /customAccountNeedsVerification\(props\.account\)/);
  assert.match(card, /@click="emit\('verify'\)"/);
  assert.match(card, /:loading="verifying"/);
  assert.match(card, /customAccountToggleBlocked\(props\.account\)/);
  assert.match(card, /:disabled="!!toggleBlockedReason"/);
  // Go-only controls stay Go-gated, and purchase/expiry tags skip Custom.
  assert.match(card, /v-if="isGo && accountIsReady\(account\)"/);
  assert.match(card, /!isZen && !isCustom && accountIsReady\(account\)/);
  // The persistent warning carries the administrator-trust risk copy.
  assert.match(card, /目标端点由管理员自行选择并负责/);
});

test("the form binds every capability to its selected protocol and reserves verification for the saved card", () => {
  assert.match(form, /目标端点由管理员自行选择并负责/);
  assert.doesNotMatch(form, /\$emit\('verify'\)/);
  assert.match(form, /\{\{ capabilityProtocol \}\}/);
  assert.match(form, /capability\.protocol = protocol/);
  // Base URL validation delegates to the shared trusted-URL helper.
  assert.match(form, /customBaseUrlIssue\(value \?\? ""\)/);
  // Edit mode forwards base URL and capabilities in the payload.
  assert.match(form, /payload\.base_url = form\.value\.baseUrl\.trim\(\)/);
  assert.match(form, /payload\.model_capabilities = form\.value\.modelCapabilities\.map/);
  // Protocol and auth scheme remain immutable after create.
  assert.match(form, /:disabled="fieldImmutableAfterCreate\('upstream_protocol'\)"/);
  assert.match(form, /:disabled="fieldImmutableAfterCreate\('auth_scheme'\)"/);
});

test("model discovery ignores responses after the form context changes", () => {
  assert.match(form, /let discoveryGeneration = 0/);
  assert.match(form, /\{ flush: "sync" \}/);
  assert.match(form, /generation !== discoveryGeneration \|\| !modelDiscoveryContextMatches\(context\)/);
  assert.match(form, /generation === discoveryGeneration && modelDiscoveryContextMatches\(context\)/);
});
