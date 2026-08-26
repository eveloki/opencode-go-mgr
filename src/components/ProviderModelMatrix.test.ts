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

test("ProviderModelMatrix emits batch overrides for row and column actions", async () => {
  const source = await readFile(new URL("./ProviderModelMatrix.vue", import.meta.url), "utf8");
  assert.match(source, /emit\("update:overrides"/);
  assert.match(source, /makeOverrides\(/);
  assert.match(source, /applyRowBatch\(/);
  assert.match(source, /applyColumnBatch\(/);
  assert.match(source, /rowBatchOptions/);
  assert.match(source, /columnBatchOptions/);
});

test("ProviderModelMatrix disables force_on when the cell is not supported", async () => {
  const source = await readFile(new URL("./ProviderModelMatrix.vue", import.meta.url), "utf8");
  assert.match(source, /:disabled="!canForceOn\(modelId, protocol\)"/);
  assert.match(source, /cellEvidence\(modelId, protocol\)\?\.available === true/);
});

test("ProviderModelMatrix hides the probe button for custom endpoint scopes", async () => {
  const source = await readFile(new URL("./ProviderModelMatrix.vue", import.meta.url), "utf8");
  assert.match(source, /v-if="scope\.scope_kind !== 'custom_endpoint'"/);
  assert.match(source, /runRowProbe/);
  assert.match(source, /probeAccounts/);
});

test("ProviderModelMatrix surfaces a forced badge and status label per cell", async () => {
  const source = await readFile(new URL("./ProviderModelMatrix.vue", import.meta.url), "utf8");
  assert.match(source, /class="override-badge"/);
  assert.match(source, /class="status-label"/);
  assert.match(source, /status-label--\$\{cellStatus\(/);
  assert.match(source, /protocolEvidenceStatus/);
});
