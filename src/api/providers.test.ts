import assert from "node:assert/strict";
import test from "node:test";
import { createPinia, setActivePinia } from "pinia";
import { providerApi } from "./providers.ts";
import { useControlPlaneStore } from "../stores/controlPlane.ts";

interface RequestRecord {
  url: string;
  method: string;
  body: Record<string, unknown> | null;
}

function installFetch(responder: (request: RequestRecord) => object): RequestRecord[] {
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: { location: { pathname: "/dashboard" }, dispatchEvent() {} },
  });
  const requests: RequestRecord[] = [];
  Object.defineProperty(globalThis, "fetch", {
    configurable: true,
    value: async (input: string, init: RequestInit = {}) => {
      const request = {
        url: input,
        method: init.method ?? "GET",
        body: init.body ? JSON.parse(String(init.body)) as Record<string, unknown> : null,
      };
      requests.push(request);
      return new Response(JSON.stringify(responder(request)), {
        headers: { "Content-Type": "application/json" },
      });
    },
  });
  return requests;
}

test("Go protocol probe sends the selected accountId in the frozen V3 body", async () => {
  setActivePinia(createPinia());
  useControlPlaneStore().sync({ revision: 12, processGeneration: 42, pricingRevision: "p1" });
  const requests = installFetch(({ url }) => {
    if (url.endsWith("/accounts/go-account-2")) {
      return { id: "go-account-2", providerId: "opencode", revision: 12, processGeneration: 42 };
    }
    if (url.endsWith("/providers/opencode/protocol-probes")) {
      return {
        accountId: "go-account-2",
        providerId: "opencode",
        modelId: "gpt-5.6-luna",
        results: [{ protocol: "responses", success: true, skipped: false, error: null }],
        contract: null,
        revision: 12,
        processGeneration: 42,
        pricingRevision: "p1",
      };
    }
    throw new Error(`unexpected request ${url}`);
  });

  const result = await providerApi.runProtocolProbes("go-account-2", {
    model_id: "gpt-5.6-luna",
    protocols: ["responses"],
  });

  assert.equal(result.account_id, "go-account-2");
  assert.deepEqual(requests[1], {
    url: "/dashboard/api/v3/providers/opencode/protocol-probes",
    method: "POST",
    body: {
      accountId: "go-account-2",
      modelId: "gpt-5.6-luna",
      protocols: ["responses"],
      expectedRevision: 12,
      processGeneration: 42,
    },
  });
});

test("Custom endpoint protocol probe and switch are blocked before unsupported writes", async () => {
  setActivePinia(createPinia());
  useControlPlaneStore().sync({ revision: 8, processGeneration: 42, pricingRevision: "p1" });
  const requests = installFetch(({ url }) => {
    if (url.endsWith("/accounts/custom-1")) {
      return { id: "custom-1", providerId: "custom", revision: 8, processGeneration: 42 };
    }
    throw new Error(`unsupported request ${url}`);
  });

  await assert.rejects(
    () => providerApi.runProtocolProbes("custom-1", {
      model_id: "Org/Model",
      protocols: ["chat_completions"],
    }),
    /尚未纳入 Dashboard V3 合同/,
  );
  await assert.rejects(
    () => providerApi.updateProviderContractProtocol(
      "custom_endpoint",
      "custom-1",
      "chat_completions",
      { enabled: false },
    ),
    /尚未纳入 Dashboard V3 合同/,
  );
  assert.deepEqual(requests.map(({ method, url }) => ({ method, url })), [
    { method: "GET", url: "/dashboard/api/v3/accounts/custom-1" },
  ]);
});
