import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const providers = readFileSync(new URL("./Providers.vue", import.meta.url), "utf8");
const protocolSwitches = readFileSync(new URL("../components/ProviderProtocolSwitches.vue", import.meta.url), "utf8");
const probePanel = readFileSync(new URL("../components/ProviderProbePanel.vue", import.meta.url), "utf8");
const modelList = readFileSync(new URL("../components/ProviderModelList.vue", import.meta.url), "utf8");
const catalog = readFileSync(new URL("../components/PricingCatalog.vue", import.meta.url), "utf8");

test("Providers reads and mutates contracts through its store while explicit actions stay page-local", () => {
  assert.match(providers, /useProvidersStore\(\)/);
  assert.match(providers, /providersStore\.loadContracts\(\)/);
  assert.match(providers, /providersStore\.loadCatalog\(\)/);
  assert.match(providers, /providersStore\.putProtocolSwitch\(/);
  assert.match(providers, /providerApi\.refreshProviderModels\(/);
  assert.match(providers, /providerApi\.runProtocolProbes\(/);
  assert.match(providers, /error\.status === 409/);
  assert.match(providers, /scope\.scope_kind === "custom_endpoint"/);
  assert.doesNotMatch(providers, /getProviderModels\(/);
  assert.doesNotMatch(providers, /getProviderModelCapabilities/);
  assert.doesNotMatch(providers, /dashboardApi\.testAccount/);
  assert.doesNotMatch(providers, /\/accounts\/\$\{.*\}\/test/);
  assert.match(providers, /onMounted\(\(\) => \{\s*window\.addEventListener\("popstate", onPopState\);\s*void loadContracts\(\);/);
  assert.doesNotMatch(providers, /onMounted\([\s\S]*runProbe/);
  assert.doesNotMatch(providers, /onMounted\([\s\S]*refreshCatalog/);
  assert.doesNotMatch(providers, /onActivated\([\s\S]*runProbe/);
  assert.doesNotMatch(providers, /onActivated\([\s\S]*refreshCatalog/);
  assert.doesNotMatch(providers, /setInterval/);
});

test("Providers keeps last-good contracts while actions fail and distinguishes page vs action errors", () => {
  assert.match(providers, /v-else-if="loadError && !contracts"/);
  assert.match(providers, /v-if="loadError && contracts"/);
  assert.match(providers, /catalogRefreshError/);
  assert.match(providers, /protocolError/);
  assert.match(providers, /probeError/);
  assert.match(providers, /await loadContracts\(\{ retain: true \}\)/);
  assert.match(providers, /aria-live="polite"/);
});

test("Providers catalog refresh toasts success only after contracts GET replaces the snapshot", () => {
  const load = providers.slice(
    providers.indexOf("async function loadContracts"),
    providers.indexOf("async function updateProtocol"),
  );
  const refresh = providers.slice(
    providers.indexOf("async function refreshCatalog"),
    providers.indexOf("async function runProbe"),
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

test("protocol switches, catalog refresh, and probes stay explicit and unique", () => {
  assert.match(protocolSwitches, /<fieldset class="protocol-policy"/);
  assert.match(protocolSwitches, /t\('启用 \{protocol\}'/);
  assert.match(providers, /catalogRefreshSupported\(activeScope\)/);
  assert.match(providers, /protocolProbeSupported\(activeScope\)/);
  assert.match(probePanel, /t\("我了解这会发送真实最小请求，并可能消耗额度"\)/);
  assert.match(probePanel, /t\("发送探测"\)/);
  assert.match(probePanel, /uniqueProtocols/);
  assert.match(providers, /if \(!scope \|\| !protocolProbeSupported\(scope\) \|\| probeInFlight\.value\) return/);
  assert.match(modelList, /t\("暂无模型合约"\)/);
  assert.match(modelList, /protocolEvidenceStatus/);
});

test("Providers pricing is filtered to the active provider and 390px layout does not require horizontal scrolling", () => {
  assert.match(providers, /<PricingCatalog :provider-id="activeScope.provider_id" \/>/);
  assert.match(catalog, /buildScopedPlanPricingGroups\(props\.providerId/);
  assert.match(providers, /providers-mobile-nav/);
  assert.match(providers, /@media \(max-width: 390px\)/);
  assert.match(providers, /overflow-x: hidden/);
  assert.match(providers, /min-width: 0/);
  assert.match(providers, /@media \(max-width: 720px\)/);
});
