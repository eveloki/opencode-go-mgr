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
  assert.match(providers, /providerApi\.refreshProviderModels\(/);
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

test("Providers catalog refresh toasts success only after contracts GET replaces the snapshot", () => {
  const load = providers.slice(
    providers.indexOf("async function loadContracts"),
    providers.indexOf("async function refreshCatalog"),
  );
  const refresh = providers.slice(
    providers.indexOf("async function refreshCatalog"),
    providers.indexOf("async function updateOverrides"),
  );
  assert.match(load, /Promise<\{ ok: boolean; error: string \}>/);
  assert.match(load, /return \{ ok: true, error: "" \}/);
  assert.match(load, /return \{ ok: false, error \}/);
  assert.match(refresh, /const refreshed = await providerApi\.refreshProviderModels/);
  assert.match(refresh, /const loaded = await loadContracts\(\{ retain: true \}\)/);
  assert.match(refresh, /if \(!loaded\.ok \|\| loadError\.value\)/);
  assert.match(refresh, /catalogRefreshError\.value = loaded\.error \|\| loadError\.value/);
  assert.match(refresh, /message\.error\(t\("刷新模型目录失败: \{error\}"/);
  assert.match(refresh, /message\.success\(t\("已刷新模型目录"\)\)/);
  const loadIdx = refresh.indexOf("const loaded = await loadContracts");
  const failIdx = refresh.indexOf("if (!loaded.ok || loadError.value)");
  const errorIdx = refresh.indexOf("message.error");
  const successIdx = refresh.indexOf('message.success(t("已刷新模型目录")');
  assert.ok(loadIdx >= 0 && failIdx > loadIdx && errorIdx > failIdx && successIdx > failIdx);
  assert.doesNotMatch(refresh, /onMounted|onActivated|setInterval/);
});

test("override mutations are batched and conflict-aware", () => {
  const update = providers.slice(
    providers.indexOf("async function updateOverrides"),
    providers.indexOf("async function runModelProbe"),
  );
  assert.match(update, /putModelProtocolOverrides\(/);
  assert.match(update, /overrides: ModelProtocolOverrideUpdate\[\]/);
  assert.match(update, /contracts\.value = normalizeProviderContractsResponse\(response\)/);
  assert.match(update, /error instanceof DashboardRequestError && error\.status === 409/);
  assert.match(update, /await loadContracts\(\{ retain: true \}\)/);
  assert.match(update, /message\.warning\(t\("供应商设置已在其他位置更新，已重新加载，请重试"\)\)/);
  assert.match(update, /message\.error\(t\("保存协议覆盖失败: \{error\}"/);
  assert.doesNotMatch(update, /onMounted|onActivated|setInterval/);
});

test("row-level probes send all three protocols and merge the returned contract", () => {
  const probe = providers.slice(
    providers.indexOf("async function runModelProbe"),
    providers.indexOf("function onPopState"),
  );
  assert.match(probe, /protocols: \[\.\.\.PROVIDER_PROTOCOLS\]/);
  assert.match(probe, /applyModelContractToResponse\(/);
  assert.match(probe, /await loadContracts\(\{ retain: true \}\)/);
  assert.match(probe, /probingModels\.value = new Set/);
  assert.match(probe, /probingModels\.value\.has\(payload\.modelId\)/);
  assert.match(probe, /message\.success\(t\("探测完成"\)\)/);
  assert.match(probe, /message\.error\(t\("探测失败: \{error\}"/);
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

test("ProviderModelMatrix renders a model-by-protocol grid with override controls and row probes", () => {
  assert.match(matrix, /matrixModels/);
  assert.match(matrix, /PROVIDER_PROTOCOLS/);
  assert.match(matrix, /n-radio-group/);
  assert.match(matrix, /value="auto"/);
  assert.match(matrix, /value="force_on"/);
  assert.match(matrix, /value="force_off"/);
  assert.match(matrix, /canForceOn\(modelId, protocol\)/);
  assert.match(matrix, /cellForced\(modelId, protocol\)/);
  assert.match(matrix, /applyRowBatch/);
  assert.match(matrix, /applyColumnBatch/);
  assert.match(matrix, /n-popconfirm/);
  assert.match(matrix, /runRowProbe/);
  assert.match(matrix, /scope\.scope_kind !== 'custom_endpoint'/);
  assert.match(matrix, /protocolEvidenceStatus/);
  assert.match(matrix, /statusKeys/);
});
