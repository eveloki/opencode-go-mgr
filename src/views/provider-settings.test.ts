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
  display_name: `${provider_id} ${offering_id}`,
  display_family: provider_id,
  creation_availability: "available",
  verification_policy: "not_required",
  verification_runtime_availability: "optional",
  routable: true,
  managed_registration: provider_id === "opencode",
  pricing_availability: "available",
  usage_availability: "available",
  quota_unit: "usd",
  model_source: "test",
  auth_schemes: ["bearer"],
  upstream_protocols: ["chat_completions"],
  form_fields: [],
  model_aliases: [],
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
  assert.match(accounts, /const enabled = patch\.enabled \?\? account\.enabled/);
  assert.match(accounts, /free_alias_enabled: enabled && \(patch\.free_alias_enabled \?\? account\.free_alias_enabled\)/);
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
  // Loading and error states with retry; every catalog plan has an explicit
  // unavailable state, so an empty catalog does not need a fabricated card.
  assert.match(catalog, /role="status"/);
  assert.match(catalog, /aria-live="polite"/);
  assert.match(catalog, /加载服务商目录失败: \{error\}/);
  assert.match(catalog, /@click="loadProviderCatalog"/);
  // Pricing is grouped by Plan via the pure grouping helper.
  assert.match(catalog, /buildPlanPricingGroups/);
  assert.match(catalog, /resolvePlanPricingDisplay/);
  assert.match(catalog, /v-for="group in planGroups"/);
  // OpenCode Go keeps the full table/edit/manual-refresh flow.
  assert.match(catalog, /onMounted\(\(\) => void loadPricing\(\)\)/);
  assert.match(catalog, /@click="requestPricingRefresh"/);
  const goBlockStart = catalog.indexOf("group.content.kind === 'opencode-go'");
  assert.notEqual(goBlockStart, -1);
  const goBlock = catalog.slice(goBlockStart, catalog.indexOf("</template>", goBlockStart));
  assert.match(goBlock, /n-data-table/);
  // The exhaustive state copy is behavior-tested in pricing-plans.test.ts;
  // the only rendered data table still belongs to OpenCode Go.
  assert.equal(catalog.match(/<n-data-table/g)?.length, 1);
});

test("account form uses the catalog display name and does not invent GOAT availability", () => {
  const accountForm = readFileSync(new URL("../components/AccountFormModal.vue", import.meta.url), "utf8");
  const accountCard = readFileSync(new URL("../components/AccountCard.vue", import.meta.url), "utf8");
  const accounts = readFileSync(new URL("./Accounts.vue", import.meta.url), "utf8");
  const chooser = readFileSync(new URL("../components/AccountAddModal.vue", import.meta.url), "utf8");

  assert.match(accountForm, /label: entry\.display_name/);
  assert.match(accountForm, /t\("添加 \{plan\} 账号"/);
  assert.match(accountForm, /'aria-label': `\$\{t\('模型 ID'\)\} \$\{index \+ 1\}`/);
  assert.match(accountForm, /:aria-label="`\$\{t\('协议'\)\} \$\{index \+ 1\}`"/);
  assert.match(accountForm, /:aria-label="`\$\{t\('删除'\)\} \$\{t\('模型能力'\)\} \$\{index \+ 1\}`"/);
  assert.match(accountForm, /:disabled="fieldImmutableAfterCreate\('upstream_protocol'\)"/);
  assert.match(accountForm, /:disabled="fieldImmutableAfterCreate\('auth_scheme'\)"/);
  assert.equal(accountForm.match(/t\("创建后不可修改"\)/g)?.length, 2);
  assert.match(accountForm, /t\(accountCreatePayloadErrorKey\(error\)\)/);
  assert.doesNotMatch(accountForm, /实验性 · 未配置/);
  assert.match(accountCard, /planLabel\(account, catalog\)/);
  assert.doesNotMatch(accountCard, /<n-tag v-if="isDraft"/);
  assert.match(accounts, /:catalog="providerCatalog"/);
  assert.match(accounts, /@import-key="openCreateModal\(OPENCODE_GO_PLAN\)"/);
  assert.match(accounts, /加载服务商目录失败: \{error\}/);
  assert.match(chooser, /t\(option\.disabledReason\)/);
  assert.match(chooser, /t\(option\.creationHint\)/);
});

test("Applications labels all model selectors as Alias-first", () => {
  const applications = readFileSync(new URL("./Applications.vue", import.meta.url), "utf8");
  assert.equal(applications.match(/t\('选择 Alias（模型 ID）'\)/g)?.length, 3);
  assert.doesNotMatch(applications, /t\('选择模型 ID'\)/);
});
