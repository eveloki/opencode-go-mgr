import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const accounts = readFileSync(new URL("./Accounts.vue", import.meta.url), "utf8");
const ordering = readFileSync(new URL("./useAccountOrder.ts", import.meta.url), "utf8");
const api = readFileSync(new URL("../api/tauri.ts", import.meta.url), "utf8");
const card = readFileSync(new URL("../components/AccountCard.vue", import.meta.url), "utf8");

test("account mutations consume and advance the shared revision", () => {
  assert.match(accounts, /expected_revision: revision/);
  assert.match(accounts, /tauriApi\.toggleAccount\(id, revision\)/);
  assert.match(accounts, /tauriApi\.deleteAccount\(id, revision\)/);
  assert.match(accounts, /settingsRevision\.value = account\.revision/);
  assert.match(ordering, /tauriApi\.reorderAccounts\([^]*revision\.value/);
  assert.match(ordering, /revision\.value = saved\[0\]\?\.revision/);
  assert.match(api, /body: jsonBody\(\{ expected_revision: expectedRevision \}\)/);
});

test("GOAT cards are fail-closed and never call legacy Go usage or ping controls", () => {
  assert.match(card, /const isGoat = computed/);
  assert.match(card, /v-else-if="isGoat" class="provider-unconfigured"/);
  assert.match(card, /v-if="isGo && accountIsReady\(account\)"/);
  assert.match(accounts, /account\.provider_id === "opencode"/);
  assert.match(accounts, /account\.offering_id === "go"/);
});
