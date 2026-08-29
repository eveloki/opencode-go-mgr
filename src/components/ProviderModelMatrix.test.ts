import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

test("ProviderModelMatrix uses a scrollable table with sticky headers", async () => {
  const source = await readFile(new URL("./ProviderModelMatrix.vue", import.meta.url), "utf8");
  assert.match(source, /<table class="matrix-table">/);
  assert.match(source, /overflow-x: auto/);
  assert.match(source, /position:\s*sticky/);
  assert.match(source, /matrix-cell--protocol-header/);
});

test("ProviderModelMatrix binds one switch per model-protocol cell to the enabled state", async () => {
  const source = await readFile(new URL("./ProviderModelMatrix.vue", import.meta.url), "utf8");
  assert.match(source, /<n-switch/);
  assert.match(source, /:value="cellEnabled\(modelId, protocol\)"/);
  assert.match(source, /cellEvidence\(modelId, protocol\)\?\.enabled === true/);
  assert.match(source, /props\.optimisticOverrides\?\.get\(cellKey\(modelId, protocol\)\)/);
  assert.match(source, /:loading="cellSaving\(modelId, protocol\)"/);
  assert.match(source, /:disabled="props\.actionLocked \|\| rowProbing\(modelId\)"/);
  assert.match(source, /\.matrix-switch \{\s*--n-rail-color-active: var\(--ocg-success\)/);
});

test("ProviderModelMatrix scopes pending override state to the affected cells", async () => {
  const source = await readFile(new URL("./ProviderModelMatrix.vue", import.meta.url), "utf8");
  assert.match(source, /modelProtocolOverrideKey\(/);
  assert.match(source, /pendingOverrideKeys\?\.has\(cellKey\(modelId, protocol\)\)/);
  assert.match(source, /columnSaving\(protocol\)/);
  assert.doesNotMatch(source, /loading\?: boolean/);
});

test("ProviderModelMatrix renders and probes only current catalog models", async () => {
  const source = await readFile(new URL("./ProviderModelMatrix.vue", import.meta.url), "utf8");
  assert.match(source, /new Set\(props\.scope\.catalog\.models\)/);
  assert.doesNotMatch(source, /for \(const model of props\.scope\.models\) ids\.add/);
  assert.match(source, /overridesSaving\(\) \|\| rowProbing\(modelId\)/);
});

test("ProviderModelMatrix presents canonical aliases and derives an all-disabled provider state", async () => {
  const source = await readFile(new URL("./ProviderModelMatrix.vue", import.meta.url), "utf8");
  assert.match(source, /modelContract\(modelId\)\?\.alias\?\.trim\(\)/);
  assert.match(source, /modelAlias\(modelId\) \|\| modelId/);
  assert.match(source, /modelAlias\(modelId\) !== modelId/);
  assert.match(source, /const providerDisabled = computed/);
  assert.match(source, /matrixModels\.value\.length > 0/);
  assert.match(source, /cellEnabled\(modelId, protocol\)/);
  assert.match(source, /t\("全部供应商协议已关闭"\)/);
});

test("ProviderModelMatrix shows all provider protocols while limiting Custom to declared evidence", async () => {
  const source = await readFile(new URL("./ProviderModelMatrix.vue", import.meta.url), "utf8");
  assert.match(source, /v-for="protocol in matrixProtocols"/);
  assert.match(source, /scope\.scope_kind !== "custom_endpoint"/);
  assert.doesNotMatch(source, /scope\.provider_id !== "command-code"/);
  assert.match(source, /model\.protocols\[protocol\]\?\.available === true/);
  assert.doesNotMatch(source, /v-for="protocol in PROVIDER_PROTOCOLS"/);
});

test("ProviderModelMatrix emits force_on or force_off on switch toggle, never auto", async () => {
  const source = await readFile(new URL("./ProviderModelMatrix.vue", import.meta.url), "utf8");
  assert.match(source, /updateSingle\(modelId, protocol, on \? 'force_on' : 'force_off'\)/);
  assert.match(source, /emit\("update:overrides"/);
  const singleStates = source.match(/updateSingle\(modelId, protocol, [^)]+\)/g) ?? [];
  assert.ok(singleStates.length > 0);
  for (const call of singleStates) assert.ok(!call.includes("'auto'"), `unexpected auto state in ${call}`);
  assert.doesNotMatch(source, /canForceOn/);
});

test("ProviderModelMatrix batch actions set whole columns on or off", async () => {
  const source = await readFile(new URL("./ProviderModelMatrix.vue", import.meta.url), "utf8");
  assert.match(source, /makeOverrides\(/);
  assert.match(source, /applyColumnBatch\(/);
  assert.match(source, /columnBatchOptions/);
  assert.match(source, /\{ key: "force_on", label: t\("全部开启"\) \}/);
  assert.match(source, /\{ key: "force_off", label: t\("全部关闭"\) \}/);
  assert.doesNotMatch(source, /rowBatchOptions/);
  assert.doesNotMatch(source, /applyRowBatch/);
  assert.doesNotMatch(source, /\{ key: "auto"/);
});

test("ProviderModelMatrix renders probe controls only when the Provider supports probes", async () => {
  const source = await readFile(new URL("./ProviderModelMatrix.vue", import.meta.url), "utf8");
  assert.match(source, /ReloadOutlined/);
  assert.match(source, /:aria-label="t\('测试'\)"/);
  assert.match(source, /:loading="rowProbing\(modelId\)"/);
  assert.match(source, /const probeSupported = computed\(\(\) => props\.scope\.card\.protocol_probe\)/);
  assert.match(source, /v-if="probeSupported"/);
  assert.match(source, /if \(!probeSupported\.value\) return/);
  assert.match(source, /n-popconfirm/);
  assert.match(source, /runRowProbe/);
  assert.match(source, /emit\("probe", \{ modelId \}\)/);
  assert.doesNotMatch(source, /probeAccounts|accountId/);
});

test("ProviderModelMatrix has no expand rows, dots, dropdown editors, or hint remnants", async () => {
  const source = await readFile(new URL("./ProviderModelMatrix.vue", import.meta.url), "utf8");
  assert.doesNotMatch(source, /expandedModel/);
  assert.doesNotMatch(source, /toggleRow/);
  assert.doesNotMatch(source, /status-dot/);
  assert.doesNotMatch(source, /overrideOptions/);
  assert.doesNotMatch(source, /overrideStateLabel/);
  assert.doesNotMatch(source, /rowHintNeeded/);
  assert.doesNotMatch(source, /cellHintNeeded/);
  assert.doesNotMatch(source, /status-label/);
  assert.doesNotMatch(source, /override-badge/);
  assert.doesNotMatch(source, /n-radio-group/);
  assert.doesNotMatch(source, /无可用证据/);
});
