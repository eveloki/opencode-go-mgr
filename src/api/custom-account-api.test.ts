import assert from "node:assert/strict";
import test from "node:test";
import { tauriApi } from "./tauri.ts";

/**
 * Route-shape contract for the Custom API account endpoints:
 * - POST /accounts/{id}/verify with an optional revision guard returns the
 *   Account (verification never enables);
 * - PUT /accounts/{id}/custom-config sends the flattened config plus the
 *   revision guard;
 * - PUT /accounts/{id}/model-capabilities sends `{ capabilities }` plus the
 *   revision guard.
 */

function mockDashboardFetch(
  handler: (url: string, init: RequestInit) => unknown,
): Array<{ url: string; method: string; body: Record<string, unknown> | null }> {
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: { location: { pathname: "/dashboard" }, dispatchEvent() {} },
  });
  const requests: Array<{ url: string; method: string; body: Record<string, unknown> | null }> = [];
  Object.defineProperty(globalThis, "fetch", {
    configurable: true,
    value: async (input: string, init: RequestInit = {}) => {
      requests.push({
        url: input,
        method: init.method ?? "GET",
        body: init.body ? JSON.parse(String(init.body)) as Record<string, unknown> : null,
      });
      return new Response(JSON.stringify(handler(input, init)), {
        headers: { "Content-Type": "application/json" },
      });
    },
  });
  return requests;
}

test("verify posts to the verify route with the revision guard", async () => {
  const requests = mockDashboardFetch(() => ({ id: "custom-1", verification_status: "verified" }));

  await tauriApi.verifyAccountConnection("custom-1", 7);

  assert.equal(requests[0]?.url, "/dashboard/api/accounts/custom-1/verify");
  assert.equal(requests[0]?.method, "POST");
  assert.deepEqual(requests[0]?.body, { expected_revision: 7 });
});

test("verify omits the body when no revision is known", async () => {
  const requests = mockDashboardFetch(() => ({ id: "custom-1" }));

  await tauriApi.verifyAccountConnection("custom-1");

  assert.equal(requests[0]?.url, "/dashboard/api/accounts/custom-1/verify");
  assert.equal(requests[0]?.method, "POST");
  assert.equal(requests[0]?.body, null);
});

test("custom config PUT sends the flattened config with the revision guard", async () => {
  const requests = mockDashboardFetch(() => ({ id: "custom-1" }));

  await tauriApi.updateAccountCustomConfig("custom-1", {
    base_url: "http://192.168.1.10:8080/v1",
    upstream_protocol: "chat_completions",
    auth_scheme: "bearer",
  }, 9);

  assert.equal(requests[0]?.url, "/dashboard/api/accounts/custom-1/custom-config");
  assert.equal(requests[0]?.method, "PUT");
  assert.deepEqual(requests[0]?.body, {
    base_url: "http://192.168.1.10:8080/v1",
    upstream_protocol: "chat_completions",
    auth_scheme: "bearer",
    expected_revision: 9,
  });
});

test("model capabilities PUT wraps the list and keeps exact model IDs and order", async () => {
  const requests = mockDashboardFetch(() => ({ id: "custom-1" }));

  await tauriApi.updateAccountModelCapabilities("custom-1", [
    { model_id: "Org/Model-B", protocol: "chat_completions", source: "manual" },
    { model_id: "custom_model.a", protocol: "chat_completions", source: "manual" },
  ], 10);

  assert.equal(requests[0]?.url, "/dashboard/api/accounts/custom-1/model-capabilities");
  assert.equal(requests[0]?.method, "PUT");
  assert.deepEqual(requests[0]?.body, {
    capabilities: [
      { model_id: "Org/Model-B", protocol: "chat_completions", source: "manual" },
      { model_id: "custom_model.a", protocol: "chat_completions", source: "manual" },
    ],
    expected_revision: 10,
  });
});
