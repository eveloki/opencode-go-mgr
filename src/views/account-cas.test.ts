import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  ACCOUNT_REVISION_UNAVAILABLE_MESSAGE,
  withFreshAccountRevision,
} from "./account-cas.ts";

const accounts = readFileSync(new URL("./Accounts.vue", import.meta.url), "utf8");
const ordering = readFileSync(new URL("./useAccountOrder.ts", import.meta.url), "utf8");
const api = readFileSync(new URL("../api/tauri.ts", import.meta.url), "utf8");
const card = readFileSync(new URL("../components/AccountCard.vue", import.meta.url), "utf8");

test("account mutations require a freshly loaded shared revision", () => {
  assert.match(accounts, /expected_revision: revision/);
  assert.match(accounts, /runWithFreshSettingsRevision\(\(revision\) => tauriApi\.toggleAccount\(id, revision\)\)/);
  assert.match(accounts, /runWithFreshSettingsRevision\(\(revision\) => tauriApi\.deleteAccount\(id, revision\)\)/);
  assert.match(accounts, /async function runWithFreshSettingsRevision[\s\S]*?tauriApi\.getSettings\(\)/);
  assert.match(accounts, /async function reloadAfterControlPlaneConflict[\s\S]*?tauriApi\.getSettings\(\)[\s\S]*?tauriApi\.getAccounts\(\)/);
  assert.match(accounts, /settingsRevision\.value = account\.revision/);
  assert.match(ordering, /runWithFreshRevision\(\(freshRevision\)[\s\S]*?tauriApi\.reorderAccounts\([^]*freshRevision/);
  assert.match(ordering, /revision\.value = saved\[0\]\?\.revision/);
  assert.match(api, /body: jsonBody\(\{ expected_revision: expectedRevision \}\)/);
});

test("a missing fresh revision aborts the mutation before it can send a request", async () => {
  let mutationCalls = 0;
  await assert.rejects(
    withFreshAccountRevision(async () => null, async () => {
      mutationCalls += 1;
    }),
    new RegExp(ACCOUNT_REVISION_UNAVAILABLE_MESSAGE),
  );
  assert.equal(mutationCalls, 0);
});

test("GOAT cards are fail-closed and never call legacy Go usage or ping controls", () => {
  assert.match(card, /const isGoat = computed/);
  assert.match(card, /v-else-if="isGoat" class="provider-unconfigured"/);
  assert.match(card, /v-if="isGo && accountIsReady\(account\)"/);
  assert.match(accounts, /account\.provider_id === "opencode"/);
  assert.match(accounts, /account\.offering_id === "go"/);
});
