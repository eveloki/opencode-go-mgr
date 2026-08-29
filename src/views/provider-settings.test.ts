import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import type { Account } from "../api/dashboard.ts";
import { accountStatusLabel } from "../domain/account-display.ts";
import { buildPricingOfferingSections } from "../domain/pricing-view.ts";
import {
  ZEN_FREE_ACCOUNT_ID,
  ZEN_FREE_OFFERING,
} from "../domain/account-providers.ts";
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

test("pricing sections keep the three known offerings in stable order without a catalog", () => {
  const sections = buildPricingOfferingSections(null);

  assert.deepEqual(
    sections.map(({ provider_id, offering_id, presentation }) => (
      `${provider_id}/${offering_id}:${presentation}`
    )),
    [
      "opencode/go:table",
      "command-code/goat:table",
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
  const providersApi = readFileSync(new URL("../api/providers.ts", import.meta.url), "utf8");

  assert.match(accounts, /providerApi\.updateProviderSettings\(account\.id, \{/);
  assert.match(accounts, /saveZenProviderSettings\(account, !account\.enabled\)/);
  assert.match(accounts, /expected_revision: revision/);
  assert.match(accounts, /settingsRevision\.value = result\.revision/);
  assert.match(accounts, /error\.status !== 409/);
  assert.match(accounts, /recoverAccountMutationConflict/);
  assert.match(accounts, /message\.warning\(t\("账号设置已被其他操作修改，已重新加载最新状态，请重试"\)\)/);
  // Never a generic account PATCH for the Zen Free singleton.
  assert.doesNotMatch(accounts, /setAccountFreeAlias/);
  assert.doesNotMatch(accounts, /toggle-free-alias|free_alias_enabled/);
  assert.doesNotMatch(providersApi, /free_alias_enabled/);
});

test("non-Zen accounts keep the legacy toggle endpoint", () => {
  const accounts = readFileSync(new URL("./Accounts.vue", import.meta.url), "utf8");

  assert.match(accounts, /dashboardApi\.toggleAccount\(id, revision\)/);
  assert.match(accounts, /if \(account && isZenFreeAccount\(account\)\)/);
  assert.equal(ZEN_FREE_ACCOUNT_ID, "00000000-0000-0000-0000-000000000002");
  assert.equal(ZEN_FREE_OFFERING.quota_scope, "egress-ip");
});

test("pricing catalog fetches the provider catalog explicitly and keeps the Go table intact", () => {
  const catalog = readFileSync(new URL("../components/PricingCatalog.vue", import.meta.url), "utf8");

  assert.match(catalog, /providerApi\.getProviderCatalog\(\)/);
  assert.match(catalog, /onMounted\(\(\) => void loadProviderCatalog\(\)\)/);
  assert.match(catalog, /buildScopedPlanPricingGroups/);
  assert.match(catalog, /props\.providerId/);
  assert.match(catalog, /group\.content\.snapshot\?\.source_url \?\? GOAT_PRICING_REFERENCE\.sourceUrl/);
  // No auto-refresh of the catalog or prices.
  assert.doesNotMatch(catalog, /setInterval|setTimeout/);
  // Loading and error states with retry; every catalog plan has an explicit
  // unavailable state, so an empty catalog does not need a fabricated card.
  assert.match(catalog, /role="status"/);
  assert.match(catalog, /aria-live="polite"/);
  assert.match(catalog, /加载服务商目录失败: \{error\}/);
  assert.match(catalog, /@click="loadProviderCatalog"/);
  // Pricing is grouped by Plan via the pure grouping helper.
  assert.match(catalog, /buildScopedPlanPricingGroups/);
  assert.match(catalog, /resolvePlanPricingDisplay/);
  assert.match(catalog, /v-for="group in planGroups"/);
  // OpenCode Go keeps the full table/edit/manual-refresh flow.
  assert.match(catalog, /if \(props\.providerId === "opencode"\) void loadPricing\(\)/);
  assert.match(catalog, /@click="requestPricingRefresh"/);
  const goBlockStart = catalog.indexOf("group.content.kind === 'opencode-go'");
  assert.notEqual(goBlockStart, -1);
  const goBlock = catalog.slice(goBlockStart, catalog.indexOf("</template>", goBlockStart));
  assert.match(goBlock, /n-data-table/);
  assert.doesNotMatch(goBlock, /5 小时额度|周额度|月额度/);
  // The exhaustive state copy is behavior-tested in pricing-plans.test.ts;
  // the parent catalog owns the Go table while GOAT encapsulates its matching table.
  assert.equal(catalog.match(/<n-data-table/g)?.length, 1);
});

test("pricing catalog uses one keyboard-accessible plan-family tab switcher with Go selected first", () => {
  const catalog = readFileSync(new URL("../components/PricingCatalog.vue", import.meta.url), "utf8");
  const reference = readFileSync(new URL("../components/ProviderPricingReference.vue", import.meta.url), "utf8");

  assert.match(catalog, /v-model:value="activePlanId"/);
  assert.match(catalog, /display-directive="if"/);
  assert.match(catalog, /<n-tab-pane[\s\S]*?v-for="group in planGroups"[\s\S]*?:name="group\.plan\.id"/);
  assert.match(catalog, /const activePlanId = ref<PlanId>\("opencode-go"\)/);
  assert.match(catalog, /PRICING_PLAN_DEFINITIONS/);
  assert.match(catalog, /kind="goat"/);
  assert.doesNotMatch(catalog, /kind="scnet"/);
  assert.doesNotMatch(catalog, /<section\s+v-for="group in planGroups"/);
  assert.doesNotMatch(reference, /provider-usage|used|remaining|percentage/);
  assert.doesNotMatch(reference, /SCNet Token Plan 已归档/);
  assert.doesNotMatch(reference, /当前仍是禁用草稿|实验性接入|每月 Credits/);
  // GOAT delegates to the provider pricing snapshot, never a live meter.
  assert.match(reference, /<GoatQuotaReference[\s\S]*?:snapshot="snapshot"[\s\S]*?@save-multiplier=/);
  assert.doesNotMatch(reference, /另加处理费/);
  assert.doesNotMatch(reference, /订阅制|官方来源|NTag/);
  const quota = readFileSync(new URL("../components/GoatQuotaReference.vue", import.meta.url), "utf8");
  assert.match(quota, /未知价格不会参与费用估算/);
  assert.match(quota, /GOAT_PRICING_REFERENCE\.models/);
  assert.match(quota, /class="pricing-ledger"/);
  assert.match(quota, /<n-data-table/);
  assert.match(quota, /t\("官方倍率"\)/);
  assert.match(quota, /NInputNumber/);
  assert.doesNotMatch(quota, /5 小时额度|周额度|月额度|月费|monthlyPriceUsd|monthlyCreditsUsd|model_allowance|rollingLimitsUsd/);
  assert.doesNotMatch(quota, /<table|goat-pricing-summary|goat-pricing-table-wrap/);
  assert.doesNotMatch(quota, /provider-usage|used|remaining|percentage/);
});

test("account form uses the catalog display name and does not invent GOAT availability", () => {
  const accountForm = readFileSync(new URL("../components/AccountFormModal.vue", import.meta.url), "utf8");
  const accountCard = readFileSync(new URL("../components/AccountCard.vue", import.meta.url), "utf8");
  const accounts = readFileSync(new URL("./Accounts.vue", import.meta.url), "utf8");
  const chooser = readFileSync(new URL("../components/AccountAddModal.vue", import.meta.url), "utf8");
  const providerQuota = readFileSync(new URL("../components/ProviderQuotaSummary.vue", import.meta.url), "utf8");

  assert.match(accountForm, /label: entry\.display_name/);
  assert.match(accountForm, /t\("添加 \{plan\} 账号"/);
  assert.match(accountForm, /:aria-label="t\('模型能力'\)"/);
  assert.match(accountForm, /v-model:value="form\.upstreamProtocol"/);
  assert.doesNotMatch(accountForm, /<n-checkbox-group/);
  assert.doesNotMatch(accountForm, /keyPrefixHint|Key 须以/);
  assert.match(
    accountForm,
    /const values: AccountCreateFormValues = \{[\s\S]*?notes: form\.value\.notes,[\s\S]*?\};[\s\S]*?if \(isCustomPlan\.value\) \{[\s\S]*?values\.upstream_protocol =/,
  );
  assert.match(accountForm, /if \(hasField\("purchase_date"\)\) \{[\s\S]*?values\.purchase_date =/);
  assert.match(accountForm, /class="purchase-date-control"[\s\S]*?v-if="isEdit"[\s\S]*?:disabled="isPurchaseDateToday"[\s\S]*?@click="setPurchaseDateToday"/);
  assert.match(accountForm, /purchaseDate: timestampFromLocalDate\(account\.purchase_date\),/);
  assert.match(accountForm, /function setPurchaseDateToday\(\)[\s\S]*?form\.value\.purchaseDate = timestampFromLocalDate\(localDateString\(\)\)/);
  assert.match(accountForm, /v-model:value="modelCapabilityIds"/);
  assert.match(accountForm, /multiple[\s\S]*?filterable[\s\S]*?tag[\s\S]*?max-tag-count="responsive"/);
  assert.doesNotMatch(
    accountForm,
    /const values: AccountCreateFormValues = \{[\s\S]*?upstream_protocol: form\.value\.upstreamProtocol[\s\S]*?\};/,
  );
  assert.doesNotMatch(accountForm, /removeCapability|addCapability|MinusOutlined/);
  assert.doesNotMatch(accountForm, /fieldImmutableAfterCreate/);
  assert.doesNotMatch(accountForm, /创建后不可修改/);
  assert.match(accountForm, /t\(accountCreatePayloadErrorKey\(error\)\)/);
  assert.match(accountForm, /path="key"[\s\S]*?class="full-width-field"/);
  assert.match(accountForm, /\.full-width-field,[\s\S]*?grid-column: 1 \/ -1;/);
  assert.doesNotMatch(accountForm, /实验性 · 未配置/);
  assert.match(accountCard, /planLabel\(account, catalog\)/);
  assert.doesNotMatch(accountCard, /<AccountTestPopover/);
  assert.doesNotMatch(accountCard, /subscription-risk|订阅制方案：/);
  assert.match(accountCard, /warning === "endpoint-risk"/);
  assert.doesNotMatch(accountCard, /有效协议|前往供应商|contractSummary/);
  assert.match(providerQuota, /v-for="window in displayedWindows"/);
  assert.match(providerQuota, /kind\.startsWith\("minimax_"\) && kind\.endsWith\(":video"\)/);
  assert.match(providerQuota, /<strong>\{\{ usedLabel\(window\) \}\}<\/strong>/);
  assert.match(providerQuota, /:percentage="usedPercent\(window\)"/);
  assert.match(providerQuota, /usedPercent\(window\) >= 100 \? 'error' : 'default'/);
  assert.match(providerQuota, /\(window\.used \/ window\.limit_value\) \* 100/);
  assert.match(providerQuota, /window\.unit === "percent" \|\| window\.window_kind\.startsWith\("kimi_"\)/);
  assert.doesNotMatch(providerQuota, /remainingPercent|remainingLabel/);
  assert.match(providerQuota, /:height="8"/);
  assert.match(providerQuota, /t\("\{time\}后重置"/);
  assert.match(providerQuota, /`\$\{period\} · \$\{scope\}`/);
  assert.match(providerQuota, /grid-template-columns: repeat\(auto-fit, minmax\(180px, 1fr\)\)/);
  assert.match(providerQuota, /@media \(max-width: 640px\)[\s\S]*?grid-template-columns: 1fr/);
  assert.doesNotMatch(providerQuota, /new Date\(window\.resets_at\)\.toLocaleString/);
  assert.match(accountCard, /plan\.value\?\.manual_usage_calibration \?\? false/);
  assert.match(accountCard, /grid-template-columns: repeat\(4, 40px\)/);
  assert.match(accountCard, /account-action--enabled/);
  assert.doesNotMatch(accountCard, /<n-tag v-if="isDraft"/);
  assert.match(accounts, /:catalog="providerCatalog"/);
  assert.match(accounts, /@import-key="openCreateModal\(OPENCODE_GO_PLAN\)"/);
  assert.match(accounts, /加载服务商目录失败: \{error\}/);
  assert.match(chooser, /t\(selectedOption\.disabledReason\)/);
  assert.match(chooser, /t\(selectedOption\.creationHint\)/);
  assert.match(chooser, /buildPlanChooserGroups/);
  assert.match(chooser, /account-add-layout/);
  assert.doesNotMatch(chooser, /account-add-grid/);
  assert.doesNotMatch(chooser, /GiftOutlined|"zen-free"/);
});

test("GOAT has no account Key-verification gate and keeps local quota estimates", () => {
  const card = readFileSync(new URL("../components/AccountCard.vue", import.meta.url), "utf8");
  const accounts = readFileSync(new URL("./Accounts.vue", import.meta.url), "utf8");

  // The catalog enables the shared local estimate/calibration surface, while
  // GOAT loads independently from the OpenCode Go pricing request.
  assert.match(card, /plan\.value\?\.manual_usage_calibration \?\? false/);
  const providers = readFileSync(new URL("../domain/account-providers.ts", import.meta.url), "utf8");
  const usage = readFileSync(new URL("../domain/useAccountUsage.ts", import.meta.url), "utf8");
  assert.doesNotMatch(providers, /COMMAND_CODE_GOAT_USAGE_LIMITS|window_5h: 14/);
  assert.match(usage, /providerApi\.getProviderUsage\(accountId\)/);
  assert.match(usage, /account && isCommandCodeGoatAccount\(account\)/);
  assert.match(accounts, /loaded\.some\(isCommandCodeGoatAccount\)/);
  assert.match(card, /根据 OCG 内已定价请求估算/);

  assert.doesNotMatch(card, /goatVerificationOffered|goatToggleBlocked/);
  assert.doesNotMatch(card, /验证连接成功后才能启用/);
  assert.match(accounts, /isCommandCodeGoatAccount/);
  assert.match(accounts, /@test-connection="openAccountTest\(account\.id\)"/);
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

test("Applications labels all model selectors as Alias-first", () => {
  const applications = readFileSync(new URL("./Applications.vue", import.meta.url), "utf8");
  assert.equal(applications.match(/t\('选择 Alias（模型 ID）'\)/g)?.length, 3);
  assert.doesNotMatch(applications, /t\('选择模型 ID'\)/);
});
