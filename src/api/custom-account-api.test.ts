import assert from "node:assert/strict";
import test from "node:test";
import { createPinia, setActivePinia } from "pinia";
import { dashboardApi } from "./dashboard.ts";
import { useControlPlaneStore } from "../stores/controlPlane.ts";

interface RecordedRequest {
  url: string;
  method: string;
  body: Record<string, unknown> | null;
}

function v3Account(id: string): Record<string, unknown> {
  return {
    id,
    name: "Custom",
    username: "",
    password: "",
    key: "",
    enabled: false,
    accountType: "key",
    setupStep: "ready",
    providerId: "custom",
    offeringId: "api",
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
    verificationStatus: "verified",
    connectionVerifiedAt: null,
    verificationError: null,
    planRoutable: true,
    customConfig: null,
    modelCapabilities: [],
  };
}

function installBrowser(
  responder: (request: RecordedRequest) => Response | object,
): RecordedRequest[] {
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: { location: { pathname: "/dashboard" }, dispatchEvent() {} },
  });
  const requests: RecordedRequest[] = [];
  Object.defineProperty(globalThis, "fetch", {
    configurable: true,
    value: async (input: string, init: RequestInit = {}) => {
      const request = {
        url: input,
        method: init.method ?? "GET",
        body: init.body ? JSON.parse(String(init.body)) as Record<string, unknown> : null,
      };
      requests.push(request);
      const result = responder(request);
      return result instanceof Response
        ? result
        : new Response(JSON.stringify(result), { headers: { "Content-Type": "application/json" } });
    },
  });
  return requests;
}

function setupControlPlane(revision = 7, processGeneration = 99): void {
  setActivePinia(createPinia());
  useControlPlaneStore().sync({ revision, processGeneration, pricingRevision: null });
}

test("verify posts to the verify route with CAS tokens", async () => {
  setupControlPlane(7);
  const requests = installBrowser(() => ({ account: v3Account("custom-1") }));

  await dashboardApi.verifyAccountConnection("custom-1");

  assert.equal(requests[0]?.url, "/dashboard/api/v3/accounts/custom-1/verify");
  assert.equal(requests[0]?.method, "POST");
  assert.deepEqual(requests[0]?.body, { expectedRevision: 7, processGeneration: 99 });
});

test("account model tests target one encoded account without CAS tokens", async () => {
  setupControlPlane(7);
  const requests = installBrowser(() => ({
    accountId: "account/1",
    modelId: "Org/Model-A",
    protocol: "chat_completions",
    success: true,
    httpStatus: 200,
    durationMs: 12,
    error: null,
  }));

  const result = await dashboardApi.testAccountModel("account/1", "Org/Model-A");

  assert.equal(result.success, true);
  assert.equal(requests[0]?.url, "/dashboard/api/v3/accounts/account%2F1/model-tests");
  assert.equal(requests[0]?.method, "POST");
  assert.deepEqual(requests[0]?.body, { modelId: "Org/Model-A" });
});

test("custom config PUT sends one Endpoint, protocol, and capability list with CAS tokens", async () => {
  setupControlPlane(9);
  const requests = installBrowser(() => ({ account: v3Account("custom-1") }));

  await dashboardApi.updateAccountCustomConfig("custom-1", {
    endpoint_url: "http://192.168.1.10:8080/v1/messages",
    upstream_protocol: "messages",
    model_capabilities: [{ public_model: "model-a", upstream_model: "provider/model-a", protocol: "messages", source: "manual" }],
  });

  assert.equal(requests[0]?.url, "/dashboard/api/v3/accounts/custom-1/custom-config");
  assert.equal(requests[0]?.method, "PUT");
  assert.deepEqual(requests[0]?.body, {
    endpointUrl: "http://192.168.1.10:8080/v1/messages",
    upstreamProtocol: "messages",
    modelCapabilities: [{ publicModel: "model-a", upstreamModel: "provider/model-a", protocol: "messages", source: "manual" }],
    expectedRevision: 9,
    processGeneration: 99,
  });
});

test("model capabilities PUT wraps the list and keeps exact model IDs and order", async () => {
  setupControlPlane(10);
  const requests = installBrowser(() => ({ account: v3Account("custom-1") }));

  await dashboardApi.updateAccountModelCapabilities("custom-1", [
    { public_model: "Org/Model-B", upstream_model: "vendor/model-b", protocol: "chat_completions", source: "manual" },
    { public_model: "custom_model.a", upstream_model: "vendor/model-b", protocol: "chat_completions", source: "manual" },
  ]);

  assert.equal(requests[0]?.url, "/dashboard/api/v3/accounts/custom-1/model-capabilities");
  assert.equal(requests[0]?.method, "PUT");
  assert.deepEqual(requests[0]?.body, {
    capabilities: [
      { publicModel: "Org/Model-B", upstreamModel: "vendor/model-b", protocol: "chat_completions", source: "manual" },
      { publicModel: "custom_model.a", upstreamModel: "vendor/model-b", protocol: "chat_completions", source: "manual" },
    ],
    expectedRevision: 10,
    processGeneration: 99,
  });
});

test("model discovery posts only the transient form fields to its protected route", async () => {
  setupControlPlane(1);
  const requests = installBrowser(() => ({ models: ["model-a"], truncated: false }));

  await dashboardApi.discoverCustomModels({
    endpoint_url: "https://api.example.com/v1/messages",
    upstream_protocol: "messages",
    api_key: "new-key",
    account_id: "custom-1",
  });

  assert.equal(requests[0]?.url, "/dashboard/api/v3/custom/models/discover");
  assert.equal(requests[0]?.method, "POST");
  assert.deepEqual(requests[0]?.body, {
    endpointUrl: "https://api.example.com/v1/messages",
    upstreamProtocol: "messages",
    apiKey: "new-key",
    accountId: "custom-1",
  });
});
