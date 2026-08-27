import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const providers = readFileSync(new URL("./Providers.vue", import.meta.url), "utf8");
const matrix = readFileSync(new URL("../components/ProviderModelMatrix.vue", import.meta.url), "utf8");
const catalog = readFileSync(new URL("../components/PricingCatalog.vue", import.meta.url), "utf8");

test("Providers reads and mutates contracts through its store while explicit actions stay page-local", () => {
  assert.match(providers, /useProvidersStore\(\)/);
  assert.match(providers, /providersStore\.loadContracts\(\)/);
  assert.match(providers, /providersStore\.loadCatalog\(\)/);
  assert.match(providers, /providersStore\.putModelProtocolOverrides\(/);
  assert.match(providers, /providersStore\.refreshContractCatalog\(/);
  assert.match(providers, /providerApi\.runProtocolProbes\(/);
  assert.match(providers, /error\.status === 409/);
  assert.match(providers, /<ProviderModelMatrix/);
  assert.match(providers, /<PricingCatalog/);
  assert.doesNotMatch(providers, /ProviderProtocolSwitches/);
  assert.doesNotMatch(providers, /ProviderProbePanel/);
  assert.doesNotMatch(providers, /ProviderModelList/);
});

test("Providers keeps last-good contracts while actions fail and distinguishes page vs action errors", () => {
  assert.match(providers, /v-else-if="loadError && !contracts"/);
  assert.match(providers, /v-if="loadError && contracts"/);
  assert.match(providers, /catalogRefreshError/);
  assert.match(providers, /matrixError/);
  assert.match(providers, /probeError/);
  assert.match(providers, /await loadContracts\(\{ retain: true \}\)/);
  assert.match(providers, /aria-live="polite"/);
});

test("catalog refresh is scope-based, account-free, and adopts the returned contract", () => {
  const refresh = providers.slice(
    providers.indexOf("async function refreshCatalog"),
    providers.indexOf("type OverridePayload"),
  );
  assert.match(refresh, /refreshContractCatalog\(scope\.scope_kind, scope\.scope_id\)/);
  assert.match(refresh, /contracts\.value = normalizeProviderContractsResponse\(refreshed\)/);
  assert.match(refresh, /applyScopeFromQuery\(\)/);
  assert.doesNotMatch(refresh, /refreshProviderModels|refreshAccount|loadContracts/);
  assert.match(refresh, /message\.error\(t\("刷新模型目录失败: \{error\}"/);
  assert.match(refresh, /message\.success\(t\("已刷新模型目录"\)\)/);
  const adoptIdx = refresh.indexOf("contracts.value = normalizeProviderContractsResponse");
  const successIdx = refresh.indexOf('message.success(t("已刷新模型目录")');
  assert.ok(adoptIdx >= 0 && successIdx > adoptIdx);
  assert.doesNotMatch(refresh, /onMounted|onActivated|setInterval/);
});

test("Model catalog uses one content panel and one capability-driven refresh action", () => {
  assert.match(providers, /class="providers-catalog-head"/);
  assert.match(providers, /catalogRefreshSupported\(scope\)/);
  assert.match(providers, /v-if="catalogRefreshVisible"/);
  assert.match(providers, /t\("刷新模型目录"\)/);
  assert.doesNotMatch(providers, /refreshAccountId|refreshAccountOptions/);
  assert.doesNotMatch(providers, /选择用于刷新的账号|NFormItem|providers-overview/);
  assert.doesNotMatch(providers, /该供应商不支持刷新模型目录/);
});

test("override mutations render optimistically, serialize CAS writes, and remain conflict-aware", () => {
  const update = providers.slice(
    providers.indexOf("type OverridePayload"),
    providers.indexOf("async function runModelProbe"),
  );
  assert.match(update, /putModelProtocolOverrides\(/);
  assert.match(update, /overrides: ModelProtocolOverrideUpdate\[\]/);
  assert.match(update, /showOptimisticOverrides\(payload, sequence\)/);
  assert.match(update, /overrideQueue = overrideQueue\.then\(\(\) => persistOverrides\(payload, sequence\)\)/);
  assert.match(update, /settleOptimisticOverrides\(payload, sequence\)/);
  assert.match(update, /latestOverrideSequence\.get\(key\) !== sequence/);
  assert.match(update, /contracts\.value = normalizeProviderContractsResponse\(response\)/);
  assert.match(update, /error instanceof DashboardRequestError && error\.status === 409/);
  assert.match(update, /await loadContracts\(\{ retain: true \}\)/);
  assert.match(update, /message\.warning\(t\("供应商设置已在其他位置更新，已重新加载，请重试"\)\)/);
  assert.match(update, /message\.error\(t\("保存协议覆盖失败: \{error\}"/);
  assert.doesNotMatch(update, /onMounted|onActivated|setInterval/);
  assert.match(providers, /:action-locked="matrixActionLocked"/);
  const matrixLock = providers.slice(
    providers.indexOf("const matrixActionLocked"),
    providers.indexOf("const scopeMenuOptions"),
  );
  assert.doesNotMatch(matrixLock, /pendingOverrideKeys/);
});

test("row-level probes send all three protocols and merge the returned contract", () => {
  const probe = providers.slice(
    providers.indexOf("async function runModelProbe"),
    providers.indexOf("function onPopState"),
  );
  assert.match(probe, /protocols: \[\.\.\.PROVIDER_PROTOCOLS\]/);
  assert.match(probe, /runProtocolProbes\(scope\.provider_id/);
  assert.doesNotMatch(probe, /accountId/);
  assert.match(probe, /applyModelContractToResponse\(/);
  assert.match(probe, /await loadContracts\(\{ retain: true \}\)/);
  assert.match(probe, /probingModels\.value = new Set/);
  assert.match(probe, /probingModels\.value\.has\(payload\.modelId\)/);
  assert.match(probe, /response\.results\.filter\(\(result\) => !result\.success\)/);
  assert.match(probe, /message\.warning\(actionLive\.value\)/);
  assert.match(probe, /message\.success\(t\("探测完成"\)\)/);
  assert.match(probe, /message\.error\(t\("探测失败: \{error\}"/);
});

test("Providers presents per-protocol probe results above the matrix without a raw failure aggregate", () => {
  const summary = providers.slice(
    providers.indexOf('v-if="probeSummary"'),
    providers.indexOf("<ProviderModelMatrix"),
  );
  assert.match(summary, /probeResultStatus\(result\)/);
  assert.match(summary, /probeResultHttpStatus\(result\.error\)/);
  assert.match(summary, /probeResultMessage\(result\.error\)/);
  assert.match(summary, /probeResultUrl\(result\.error\)/);
  assert.match(providers, /\(\?:HTTP\\s\+\|returned\\s\+\)\(\\d\{3\}\)/);
  assert.match(providers, /raw\.indexOf\("\{"\)/);
  const failure = providers.slice(
    providers.indexOf("const failures = response.results.filter"),
    providers.indexOf('actionLive.value = t("探测完成")'),
  );
  assert.doesNotMatch(failure, /probeError\.value = failures/);
});

test("every provider with a dated static snapshot can expose the restore action", () => {
  assert.match(providers, /staticProtocolResetVisible/);
  assert.doesNotMatch(providers, /provider_id === "opencode"/);
  assert.match(providers, /static_protocol_snapshot_date/);
  assert.match(providers, /resetStaticModelProtocols/);
  assert.match(providers, /不会请求上游；将清除手动和探测判断/);
  assert.match(providers, /未出现的协议默认关闭/);
});

test("Providers pricing is filtered to the active provider and 390px layout does not require horizontal scrolling", () => {
  assert.match(providers, /<PricingCatalog :provider-id="activeScope\.provider_id" \/>/);
  assert.match(catalog, /buildScopedPlanPricingGroups\(props\.providerId/);
  assert.match(providers, /providers-mobile-nav/);
  assert.match(providers, /@media \(max-width: 390px\)/);
  assert.match(providers, /overflow-x: hidden/);
  assert.match(providers, /min-width: 0/);
  assert.match(providers, /@media \(max-width: 720px\)/);
});

test("Providers shows model catalog and model prices in two tabs", () => {
  assert.match(providers, /<n-tabs[^>]*v-model:value="activeTab"/);
  assert.match(providers, /<n-tab-pane[^>]*name="catalog"/);
  assert.match(providers, /<n-tab-pane[^>]*name="pricing"/);
  assert.match(providers, /:tab="t\('模型目录'\)"/);
  assert.match(providers, /:tab="t\('模型价格'\)"/);
  assert.doesNotMatch(providers, /id="provider-overview-title"/);
  assert.doesNotMatch(providers, /id="provider-protocol-title"/);
});

test("ProviderModelMatrix renders a model-by-protocol grid with override switches and row probes", () => {
  assert.match(matrix, /matrixModels/);
  assert.match(matrix, /PROVIDER_PROTOCOLS/);
  assert.match(matrix, /n-switch/);
  assert.match(matrix, /cellEnabled\(modelId, protocol\)/);
  assert.match(matrix, /on \? 'force_on' : 'force_off'/);
  assert.doesNotMatch(matrix, /canForceOn/);
  assert.doesNotMatch(matrix, /n-radio-group/);
  assert.doesNotMatch(matrix, /expandedModel/);
  assert.doesNotMatch(matrix, /status-dot/);
  assert.doesNotMatch(matrix, /overrideOptions/);
  assert.match(matrix, /applyColumnBatch/);
  assert.match(matrix, /columnBatchOptions/);
  assert.match(matrix, /n-popconfirm/);
  assert.match(matrix, /ReloadOutlined/);
  assert.match(matrix, /runRowProbe/);
  assert.match(matrix, /scope\.scope_kind !== 'custom_endpoint'/);
  assert.doesNotMatch(matrix, /scope\.provider_id !== "command-code"/);
});

test("Provider model rows use aliases and expose all-disabled state from matrix cells", () => {
  assert.match(matrix, /modelAlias\(modelId\) \|\| modelId/);
  assert.match(matrix, /modelAlias\(modelId\) !== modelId/);
  assert.match(matrix, /const providerDisabled = computed/);
  assert.match(matrix, /cellEnabled\(modelId, protocol\)/);
  assert.match(matrix, /全部供应商协议已关闭/);
});
