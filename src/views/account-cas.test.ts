import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  ACCOUNT_REVISION_UNAVAILABLE_MESSAGE,
  reconcileEditingAccount,
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

test("conflict reload keeps the edit modal for a surviving account, closes it for a deleted one", () => {
  const loaded = [{ id: "a1" }, { id: "a2" }];

  // Surviving account: fresh copy returned, caller keeps the modal open.
  assert.deepEqual(reconcileEditingAccount(loaded, "a2"), { id: "a2" });

  // Deleted account: null, caller must close the modal so it cannot morph
  // into create mode.
  assert.equal(reconcileEditingAccount(loaded, "gone"), null);
  assert.equal(reconcileEditingAccount(loaded, null), null);
  assert.equal(reconcileEditingAccount([], "a1"), null);

  // The component wires both branches: reconcile, then close on missing.
  assert.match(accounts, /reconcileEditingAccount\(loaded, editingAccount\.value\.id\)/);
  assert.match(accounts, /editingAccount\.value = stillListed;\s*\n[\s\S]*?if \(!stillListed\) showModal\.value = false;/);
});

test("GOAT cards are fail-closed and never call legacy Go usage or ping controls", () => {
  assert.match(card, /const isGoat = computed/);
  assert.match(card, /v-else-if="isGoat" class="provider-unconfigured"/);
  assert.match(card, /v-if="isGo && accountIsReady\(account\)"/);
  assert.match(accounts, /account\.provider_id === "opencode"/);
  assert.match(accounts, /account\.offering_id === "go"/);
});
