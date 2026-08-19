import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { toggleZenFreeAlias, toggleZenFreeEnabled } from "./zen-free-settings.ts";

test("Zen Free enabled switch maps Explicit and Prefer to Deny, then Deny to Explicit", () => {
  assert.deepEqual(toggleZenFreeEnabled({ enabled: true, free_alias_enabled: false }), {
    enabled: false,
    free_alias_enabled: false,
  });
  assert.deepEqual(toggleZenFreeEnabled({ enabled: true, free_alias_enabled: true }), {
    enabled: false,
    free_alias_enabled: false,
  });
  assert.deepEqual(toggleZenFreeEnabled({ enabled: false, free_alias_enabled: false }), {
    enabled: true,
    free_alias_enabled: false,
  });
});

test("Zen Free never forms an enabled=false plus free_alias_enabled=true combination", () => {
  assert.deepEqual(toggleZenFreeAlias({ enabled: false, free_alias_enabled: false }), {
    enabled: false,
    free_alias_enabled: false,
  });
  assert.deepEqual(toggleZenFreeAlias({ enabled: false, free_alias_enabled: true }), {
    enabled: false,
    free_alias_enabled: false,
  });
  const card = readFileSync(new URL("../components/AccountCard.vue", import.meta.url), "utf8");
  assert.match(card, /v-if="isZen && accountIsReady\(account\) && account\.enabled"/);
});
