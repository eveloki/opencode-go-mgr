import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const accounts = readFileSync(new URL("./Accounts.vue", import.meta.url), "utf8");
const api = readFileSync(new URL("../api/tauri.ts", import.meta.url), "utf8");

test("zen card has one enabled switch backed by the dedicated provider-settings write", () => {
  assert.match(accounts, /providerApi\.updateProviderSettings\(account\.id, \{/);
  assert.match(accounts, /saveZenProviderSettings\(account, !account\.enabled\)/);
  assert.doesNotMatch(accounts, /toggleZenFreeAlias|toggle-free-alias|free_alias_enabled/);
});

test("zen provider settings send the settings revision guard and reload on 409", () => {
  assert.match(accounts, /settingsRevision\.value = settingsResult\.value\.revision/);
  assert.match(accounts, /expected_revision: revision/);
  assert.match(accounts, /error\.status !== 409/);
  assert.match(accounts, /recoverAccountMutationConflict\(error\)/);
  assert.match(accounts, /message\.warning\(t\("账号设置已被其他操作修改，已重新加载最新状态，请重试"\)\)/);
  assert.match(accounts, /async function reloadAfterControlPlaneConflict[\s\S]*?tauriApi\.getSettings\([\s\S]*?tauriApi\.getAccounts\(/);
});

test("Zen card refreshes and renders the filtered model-to-alias catalog", () => {
  const card = readFileSync(new URL("../components/AccountCard.vue", import.meta.url), "utf8");
  assert.match(accounts, /providerApi\.refreshProviderModels\(id\)/);
  assert.match(accounts, /zenFreeModels\.value = result/);
  assert.match(card, /emit\('refresh-models'\)/);
  assert.match(card, /model\.alias/);
  assert.match(card, /model\.model_id/);
});

test("the generic account patch no longer carries the zen free alias", () => {
  assert.doesNotMatch(accounts, /setAccountFreeAlias/);
  assert.doesNotMatch(api, /setAccountFreeAlias/);
  const accountUpdate = api.slice(
    api.indexOf("export interface AccountUpdate"),
    api.indexOf("export type RoutingMode"),
  );
  assert.doesNotMatch(accountUpdate, /free_alias_enabled/);
});
