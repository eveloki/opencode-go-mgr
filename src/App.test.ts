import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const app = readFileSync(new URL("./App.vue", import.meta.url), "utf8");

test("the sidebar replaces Pricing with Providers and keeps the established order", () => {
  assert.match(app, /type ViewKey = AppViewKey/);
  assert.match(app, /providers: "供应商"/);
  assert.match(app, /<Providers v-else-if="activeKey === 'providers'" \/>/);
  assert.match(app, /import\("\.\/views\/Providers\.vue"\)/);
  assert.match(app, /\{ label: t\("供应商"\), key: "providers"/);
  assert.doesNotMatch(app, /key: "pricing"/);
  assert.doesNotMatch(app, /activeKey === 'pricing'/);
  assert.doesNotMatch(app, /views\/Pricing\.vue/);
  const menu = app.slice(app.indexOf("const menuOptions"), app.indexOf("const currentTitle"));
  assert.match(menu, /仪表盘[\s\S]*接入 Key[\s\S]*账号[\s\S]*供应商[\s\S]*应用[\s\S]*日志[\s\S]*设置/);
});

test("legacy pricing URLs migrate to providers with replaceState", () => {
  assert.match(app, /isLegacyPricingView\(raw\)/);
  assert.match(app, /window\.history\.replaceState/);
  assert.match(app, /applyAppViewSearchParams/);
  assert.match(app, /resolveAppViewKey\(raw\)/);
});

test("account-card provider deep links pass scope query fields through navigation", () => {
  const accounts = readFileSync(new URL("./views/Accounts.vue", import.meta.url), "utf8");
  assert.match(app, /<Accounts v-else-if="activeKey === 'accounts'" @navigate="selectView" \/>/);
  assert.match(app, /function selectView\(key: string, extras\?: ProviderScopeQuery\)/);
  assert.match(app, /applyAppViewSearchParams\([\s\S]*view, extras\)/);
  assert.match(accounts, /emit\("navigate", "providers", scope\)/);
  assert.match(accounts, /@open-provider="openProvider\(account\)"/);
  assert.match(app, /<KeepAlive>/);
});
