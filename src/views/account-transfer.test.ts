import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const v3 = readFileSync(new URL("../api/dashboard-v3.ts", import.meta.url), "utf8");
const api = readFileSync(new URL("../api/dashboard.ts", import.meta.url), "utf8");
const modal = readFileSync(new URL("../components/AccountTransferModal.vue", import.meta.url), "utf8");
const accounts = readFileSync(new URL("./Accounts.vue", import.meta.url), "utf8");

test("account transfer uses the frozen V3 routes and keeps import under CAS", () => {
  assert.match(v3, /"\/accounts\/transfer\/export"/);
  assert.match(v3, /"\/accounts\/transfer\/preview"/);
  assert.match(v3, /"\/accounts\/transfer\/import"[\s\S]*?withExpectation\(input, expectation\)/);
  assert.match(api, /importAccountTransfer:[\s\S]*?withCas\(\(expectation\)/);
  assert.match(api, /exportAccountTransfer: \(input: AccountExportRequest\)/);
  assert.match(api, /previewAccountTransfer: \(input: AccountImportPreviewRequest\)/);
});

test("transfer modal bounds files, clears transient secrets, and exposes import preview", () => {
  assert.match(modal, /const MAX_BUNDLE_BYTES = 4 \* 1024 \* 1024/);
  assert.match(modal, /accept="\.ocgbackup,application\/json"/);
  assert.match(modal, /bundlePassword\.value = ""/);
  assert.match(modal, /bundlePassword\.value === bundlePasswordConfirmation\.value/);
  assert.match(modal, /adminPassword\.value === adminPasswordConfirmation\.value/);
  assert.match(modal, /bundle\.value = ""/);
  assert.match(modal, /preview\.value = null/);
  assert.match(modal, /v-if="errorText" type="error"/);
  assert.match(modal, /aria-live="polite"/);
  assert.match(modal, /previewAccountTransfer/);
  assert.match(modal, /importAccountTransfer/);
  assert.match(modal, /const operationLocked = computed\(\(\) => busy\.value \|\| previewing\.value\)/);
  assert.match(modal, /const requestBundle = bundle\.value/);
  assert.match(modal, /epoch !== previewEpoch/);
  assert.match(modal, /bundle\.value === previewBundleSnapshot\.value/);
  assert.match(modal, /bundlePassword\.value === previewPasswordSnapshot\.value/);
  assert.match(modal, /session\.status \?\? await session\.loadStatus\(\)/);
  assert.match(modal, /session\.register\(adminUsername\.value\.trim\(\), adminPassword\.value\)/);
  assert.match(modal, /URL\.createObjectURL\(new Blob\(\[bundleText\]/);
  assert.doesNotMatch(modal, /console\.(?:log|error)/);
});

test("Accounts page puts import and export alongside Add and reloads after import", () => {
  assert.match(accounts, /openTransfer\('import'\)/);
  assert.match(accounts, /openTransfer\('export'\)/);
  assert.match(accounts, /<AccountTransferModal[\s\S]*?@imported="handleAccountsImported"/);
  assert.match(accounts, /async function handleAccountsImported\(count: number\)[\s\S]*?await loadAccounts\(\)/);
});
