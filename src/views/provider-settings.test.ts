import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { buildPricingOfferingSections } from "./pricing-view.ts";
import {
  ZEN_FREE_ACCOUNT_ID,
  ZEN_FREE_OFFERING,
} from "./account-providers.ts";
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
  pricing_availability: "available",
  usage_availability: "available",
});

test("pricing sections keep the three known offerings in stable order without a catalog", () => {
  const sections = buildPricingOfferingSections(null);

  assert.deepEqual(
    sections.map(({ provider_id, offering_id, presentation }) => (
      `${provider_id}/${offering_id}:${presentation}`
    )),
    [
      "opencode/go:table",
      "command-code/goat:experimental",
      "opencode-zen-free/anonymous-free:free",
    ],
  );
  assert.ok(sections.every(({ listed }) => !listed));
  assert.deepEqual(
    sections.map(({ label }) => label),
    ["OpenCode Go", "Command Code GOAT", "Zen Free"],
  );
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

test("Zen card toggles use the dedicated provider-settings call with the settings revision", () => {
  const accounts = readFileSync(new URL("./Accounts.vue", import.meta.url), "utf8");
  const api = readFileSync(new URL("../api/tauri.ts", import.meta.url), "utf8");

  assert.match(accounts, /providerApi\.updateProviderSettings\(account\.id, \{/);
  assert.match(accounts, /enabled: patch\.enabled \?\? account\.enabled/);
  assert.match(accounts, /free_alias_enabled: patch\.free_alias_enabled \?\? account\.free_alias_enabled/);
  assert.match(accounts, /expected_revision: revision/);
  assert.match(accounts, /settingsRevision\.value = result\.revision/);
  assert.match(accounts, /error\.status !== 409/);
  assert.match(accounts, /recoverAccountMutationConflict/);
  assert.match(accounts, /message\.warning\(t\("账号设置已被其他操作修改，已重新加载最新状态，请重试"\)\)/);
  // Never a generic account PATCH for the Zen Free singleton.
  assert.doesNotMatch(accounts, /setAccountFreeAlias/);
  assert.doesNotMatch(api, /free_alias_enabled\?: boolean/);
});

test("non-Zen accounts keep the legacy toggle endpoint", () => {
  const accounts = readFileSync(new URL("./Accounts.vue", import.meta.url), "utf8");

  assert.match(accounts, /tauriApi\.toggleAccount\(id, revision\)/);
  assert.match(accounts, /if \(account && isZenFreeAccount\(account\)\)/);
  assert.equal(ZEN_FREE_ACCOUNT_ID, "00000000-0000-0000-0000-000000000002");
  assert.equal(ZEN_FREE_OFFERING.quota_scope, "egress-ip");
});

test("pricing catalog fetches the provider catalog explicitly and keeps the Go table intact", () => {
  const catalog = readFileSync(new URL("../components/PricingCatalog.vue", import.meta.url), "utf8");

  assert.match(catalog, /providerApi\.getProviderCatalog\(\)/);
  assert.match(catalog, /onMounted\(\(\) => void loadProviderCatalog\(\)\)/);
  // No auto-refresh of the catalog or prices.
  assert.doesNotMatch(catalog, /setInterval|setTimeout/);
  // Loading / error / empty states with retry.
  assert.match(catalog, /role="status"/);
  assert.match(catalog, /aria-live="polite"/);
  assert.match(catalog, /加载服务商目录失败: \{error\}/);
  assert.match(catalog, /@click="loadProviderCatalog"/);
  assert.match(catalog, /服务商目录暂无数据/);
  // OpenCode Go keeps the full table/edit/manual-refresh flow.
  assert.match(catalog, /onMounted\(\(\) => void loadPricing\(\)\)/);
  assert.match(catalog, /@click="requestPricingRefresh"/);
  assert.match(catalog, /OpenCode Go 额度价格表/);
  // GOAT and Zen Free render semantic placeholders, not price tables.
  assert.match(catalog, /secondaryOfferingSections/);
  assert.match(catalog, /实验性接入，尚未配置价格目录，不展示价格表。/);
  assert.match(catalog, /零价格；额度按出口 IP 共享，429 后整条 free 通道冷却。/);
  const secondaryBlock = catalog.slice(catalog.indexOf("secondaryOfferingSections"));
  assert.doesNotMatch(secondaryBlock.slice(0, secondaryBlock.indexOf("</template>")), /n-data-table/);
});

test("GOAT is labeled experimental in the account provider selector", () => {
  const accountForm = readFileSync(new URL("../components/AccountFormModal.vue", import.meta.url), "utf8");

  assert.match(accountForm, /offering\.provider_id === "command-code"/);
  assert.match(accountForm, /t\("\{label\}（实验性 · 未配置）", \{ label: offering\.label \}\)/);
});
