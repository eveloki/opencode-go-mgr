import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const accounts = readFileSync(new URL("./Accounts.vue", import.meta.url), "utf8");
const api = readFileSync(new URL("../api/tauri.ts", import.meta.url), "utf8");

test("zen card toggles use the dedicated provider-settings write with both flags preserved", () => {
  assert.match(accounts, /providerApi\.updateProviderSettings\(account\.id, \{/);
  assert.match(accounts, /enabled: patch\.enabled \?\? account\.enabled/);
  assert.match(accounts, /free_alias_enabled: patch\.free_alias_enabled \?\? account\.free_alias_enabled/);
  assert.match(accounts, /async function toggleAccount\(id: string\) \{[\s\S]*?isZenFreeAccount\(account\)[\s\S]*?saveZenProviderSettings\(account, \{ enabled: !account\.enabled \}\)/);
  assert.match(accounts, /saveZenProviderSettings\(\s*account,\s*\{ free_alias_enabled: !account\.free_alias_enabled \}/);
});

test("zen provider settings send the settings revision guard and reload on 409", () => {
  assert.match(accounts, /settingsRevision\.value = settingsResult\.value\.revision/);
  assert.match(accounts, /expected_revision: revision/);
  assert.match(accounts, /error\.status === 409/);
  assert.match(accounts, /reloadAfterProviderSettingsConflict\(account\.id\)/);
  assert.match(accounts, /message\.warning\(t\("账号设置已被其他操作修改，已重新加载最新状态，请重试"\)\)/);
  assert.match(accounts, /async function reloadAfterProviderSettingsConflict[\s\S]*?tauriApi\.getSettings\(\)/);
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
