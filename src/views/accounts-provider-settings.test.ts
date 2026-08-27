import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const accounts = readFileSync(new URL("./Accounts.vue", import.meta.url), "utf8");
const api = readFileSync(new URL("../api/dashboard.ts", import.meta.url), "utf8");
const providers = readFileSync(new URL("../api/providers.ts", import.meta.url), "utf8");

test("zen card has one enabled switch backed by the dedicated provider-settings write", () => {
  assert.match(accounts, /providerApi\.updateProviderSettings\(account\.id, \{/);
  assert.match(accounts, /saveZenProviderSettings\(account, !account\.enabled\)/);
  assert.doesNotMatch(accounts, /toggleZenFreeAlias|toggle-free-alias|free_alias_enabled/);
  // Guard against the plan-id / route-slug (`zen-free`) being used as providerId.
  assert.match(providers, /account\.providerId !== "opencode-zen-free"/);
});

test("zen provider settings send the settings revision guard and reload on 409", () => {
  assert.match(accounts, /settingsRevision\.value = settingsResult\.value\.revision/);
  assert.match(accounts, /expected_revision: revision/);
  assert.match(accounts, /error\.status !== 409/);
  assert.match(accounts, /recoverAccountMutationConflict\(error\)/);
  assert.match(accounts, /message\.warning\(t\("账号设置已被其他操作修改，已重新加载最新状态，请重试"\)\)/);
  assert.match(accounts, /async function reloadAfterControlPlaneConflict[\s\S]*?dashboardApi\.getSettings\([\s\S]*?accountsStore\.loadPresented\(/);
});

test("Accounts keeps account-owned writes and no longer hosts supplier catalog or protocol tests", () => {
  const card = readFileSync(new URL("../components/AccountCard.vue", import.meta.url), "utf8");
  assert.doesNotMatch(accounts, /providerApi\.refreshProviderModels/);
  assert.doesNotMatch(accounts, /getProviderModels/);
  assert.doesNotMatch(accounts, /getProviderModelCapabilities/);
  assert.doesNotMatch(accounts, /runProtocolProbes/);
  assert.doesNotMatch(accounts, /dashboardApi\.testAccount/);
  assert.doesNotMatch(card, /emit\('refresh-models'\)/);
  assert.doesNotMatch(card, /<AccountTestPopover/);
  assert.match(accounts, /providerApi\.updateProviderSettings/);
  assert.match(accounts, /dashboardApi\.toggleAccount/);
  assert.match(card, /emit\('refresh-usage'\)/);
  assert.match(card, /emit\('open-provider'\)/);
  assert.match(accounts, /accountProviderScope\(account\)/);
});

test("Accounts omits a protocol summary until a contract snapshot exists and keeps last-good on GET failure", () => {
  const card = readFileSync(new URL("../components/AccountCard.vue", import.meta.url), "utf8");
  assert.match(accounts, /if \(!providerContracts\.value\) return null;/);
  assert.match(accounts, /Keep the last good snapshot/);
  assert.match(card, /v-if="contractSummary"/);
  assert.match(card, /t\("无有效协议"\)/);
});

test("the generic account patch no longer carries the zen free alias", () => {
  assert.doesNotMatch(accounts, /setAccountFreeAlias/);
  assert.doesNotMatch(api, /setAccountFreeAlias/);
  assert.doesNotMatch(api, /setAccountFreeAlias|free_alias_enabled/);
});

test("GOAT account cards no longer own model catalog access controls", () => {
  const card = readFileSync(new URL("../components/AccountCard.vue", import.meta.url), "utf8");
  assert.doesNotMatch(card, /goat_model_access|goat-model-access|NRadioButton|NRadioGroup/);
  assert.doesNotMatch(accounts, /goatModelAccessSaving|updateGoatModelAccess|goat-model-access/);
});
