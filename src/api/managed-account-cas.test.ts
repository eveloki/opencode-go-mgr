import assert from "node:assert/strict";
import test from "node:test";
import { tauriApi } from "./tauri.ts";

test("managed account writes include the supplied settings revision", async () => {
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: { location: { pathname: "/dashboard" }, dispatchEvent() {} },
  });
  const requests: unknown[] = [];
  Object.defineProperty(globalThis, "fetch", {
    configurable: true,
    value: async (_input: string, init: RequestInit = {}) => {
      requests.push(init.body ? JSON.parse(String(init.body)) : null);
      return new Response(JSON.stringify({ id: "managed-1" }), {
        headers: { "Content-Type": "application/json" },
      });
    },
  });

  await tauriApi.createManagedAccount({ name: "Managed", expected_revision: 21 });
  await tauriApi.advanceAccountSetup("managed-1", "opencode_registration", 22);
  await tauriApi.verifyManagedAccountKey("managed-1", "sk-secret", 23);
  await tauriApi.resetAccountCooldown("managed-1", 24);
  await tauriApi.resetAccountBrowserProfile("managed-1", 25);

  assert.deepEqual(requests, [
    { name: "Managed", expected_revision: 21 },
    { setup_step: "opencode_registration", expected_revision: 22 },
    { key: "sk-secret", expected_revision: 23 },
    { expected_revision: 24 },
    { expected_revision: 25 },
  ]);
});
