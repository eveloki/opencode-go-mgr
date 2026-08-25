import assert from "node:assert/strict";
import test from "node:test";
import { createPinia, setActivePinia } from "pinia";
import { dashboardApi } from "./dashboard.ts";
import { useControlPlaneStore } from "../stores/controlPlane.ts";

function v3Account(id: string, overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    id,
    name: "Managed",
    username: "note@example.com",
    password: "",
    key: "",
    enabled: true,
    accountType: "managed",
    setupStep: "google_account",
    providerId: "opencode",
    offeringId: "go",
    credentialKind: "api_key",
    quotaScope: "key",
    revision: 1,
    purchaseDate: "",
    expiresOn: "",
    cooldownUntil: null,
    cooldownGenericUntil: null,
    cooldown5hUntil: null,
    cooldownWeekUntil: null,
    cooldownMonthUntil: null,
    cooldownFreeUntil: null,
    lastError: null,
    authError: null,
    notes: "",
    usageSyncLastSuccessAt: null,
    usageSyncNextAllowedAt: null,
    createdAt: "2026-08-21T00:00:00Z",
    updatedAt: "2026-08-21T00:00:00Z",
    verificationStatus: "not_required",
    connectionVerifiedAt: null,
    verificationError: null,
    planRoutable: true,
    customConfig: null,
    modelCapabilities: [],
    acknowledgements: [],
    ...overrides,
  };
}

test("managed account writes include the current CAS tokens", async () => {
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: { location: { pathname: "/dashboard" }, dispatchEvent() {} },
  });
  setActivePinia(createPinia());
  useControlPlaneStore().sync({ revision: 21, processGeneration: 99, pricingRevision: null });

  const requests: Array<Record<string, unknown>> = [];
  Object.defineProperty(globalThis, "fetch", {
    configurable: true,
    value: async (_input: string, init: RequestInit = {}) => {
      requests.push(init.body ? JSON.parse(String(init.body)) : null);
      return new Response(JSON.stringify({ account: v3Account("managed-1") }), {
        headers: { "Content-Type": "application/json" },
      });
    },
  });

  await dashboardApi.createManagedAccount({ name: "Managed", username: "note@example.com" });
  await dashboardApi.advanceAccountSetup("managed-1", "opencode_registration");
  await dashboardApi.verifyManagedAccountKey("managed-1", "sk-secret");
  await dashboardApi.resetAccountCooldown("managed-1");
  await dashboardApi.resetAccountBrowserProfile("managed-1");

  assert.deepEqual(requests, [
    { name: "Managed", username: "note@example.com", expectedRevision: 21, processGeneration: 99 },
    { setupStep: "opencode_registration", expectedRevision: 21, processGeneration: 99 },
    { key: "sk-secret", expectedRevision: 21, processGeneration: 99 },
    { expectedRevision: 21, processGeneration: 99 },
    { expectedRevision: 21, processGeneration: 99 },
  ]);
});
