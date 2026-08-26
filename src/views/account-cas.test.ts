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
const api = readFileSync(new URL("../api/dashboard.ts", import.meta.url), "utf8");
const card = readFileSync(new URL("../components/AccountCard.vue", import.meta.url), "utf8");

test("account mutations require a freshly loaded shared revision", () => {
  assert.match(accounts, /expected_revision: revision/);
  assert.match(accounts, /runWithFreshSettingsRevision\(\(revision\) => dashboardApi\.toggleAccount\(id, revision\)\)/);
  assert.match(accounts, /runWithFreshSettingsRevision\(\(revision\) => dashboardApi\.deleteAccount\(id, revision\)\)/);
  assert.match(accounts, /async function runWithFreshSettingsRevision[\s\S]*?dashboardApi\.getSettings\(\)/);
  assert.match(accounts, /async function reloadAfterControlPlaneConflict[\s\S]*?dashboardApi\.getSettings\(\)[\s\S]*?accountsStore\.loadPresented\(\)/);
  assert.match(accounts, /useAccountsStore\(\)/);
  assert.match(accounts, /settingsRevision\.value = account\.revision/);
  assert.match(ordering, /runWithFreshRevision\(\(freshRevision\)[\s\S]*?dashboardApi\.reorderAccounts\([^]*freshRevision/);
  assert.match(ordering, /revision\.value = saved\[0\]\?\.revision/);
  assert.match(api, /return controlPlane\.runMutation\(run\)/);
  assert.match(api, /dashboardV3\.deleteAccount\(id, expectation\)/);
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

test("unroutable cards are fail-closed without provider-specific draft branches", () => {
  assert.match(card, /const isDraft = computed[\s\S]*?!props\.account\.plan_routable/);
  assert.match(card, /v-else-if="isDraft" class="provider-unconfigured"/);
  assert.match(card, /v-if="isGo && accountIsReady\(account\)"/);
  assert.match(card, /accountRoutingDraftDescription\(props\.account\)/);
  assert.doesNotMatch(card, /provider_id === "command-code"|provider_id === "custom"/);
});
