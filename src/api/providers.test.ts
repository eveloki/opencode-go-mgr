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

test("Go and GOAT model refresh use the selected account on the provider route", async () => {
  setActivePinia(createPinia());
  useControlPlaneStore().sync({ revision: 12, processGeneration: 42, pricingRevision: "p1" });
  const providers: Record<string, string> = {
    "go-account": "opencode",
    "goat-account": "command-code",
  };
  const requests = installFetch(({ url, method }) => {
    const accountId = Object.keys(providers).find((id) => url.endsWith(`/accounts/${id}`));
    if (accountId) {
      return {
        id: accountId,
        providerId: providers[accountId],
        revision: 12,
        processGeneration: 42,
      };
    }
    if (url.includes("/models/refresh") && method === "POST") {
      const providerId = url.includes("/providers/opencode/") ? "opencode" : "command-code";
      const body = requests.at(-1)!.body!;
      return {
        providerId,
        accountId: body.accountId,
        models: ["model-one"],
        refreshedAt: "2026-08-24T00:00:00Z",
        sourceUrl: "https://example.test/v1/models",
        revision: 12,
        processGeneration: 42,
        pricingRevision: "p1",
      };
    }
    throw new Error(`unexpected request ${url}`);
  });

  await providerApi.refreshProviderModels("go-account");
  await providerApi.refreshProviderModels("goat-account");

  assert.deepEqual(requests.filter(({ url }) => url.endsWith("/models/refresh")), [
    {
      url: "/dashboard/api/v3/providers/opencode/models/refresh",
      method: "POST",
      body: { accountId: "go-account", expectedRevision: 12, processGeneration: 42 },
    },
    {
      url: "/dashboard/api/v3/providers/command-code/models/refresh",
      method: "POST",
      body: { accountId: "goat-account", expectedRevision: 12, processGeneration: 42 },
    },
  ]);
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

const ZEN_FREE_ACCOUNT_ID = "00000000-0000-0000-0000-000000000002";

function zenFreeAccountDto(overrides: Record<string, unknown> = {}) {
  return {
    id: ZEN_FREE_ACCOUNT_ID,
    name: "OpenCode Zen Free",
    username: null,
    enabled: true,
    accountType: "key",
    setupStep: "ready",
    providerId: "opencode-zen-free",
    offeringId: "anonymous-free",
    credentialKind: "none",
    quotaScope: "egress-ip",
    revision: 12,
    processGeneration: 42,
    purchaseDate: "2026-01-01",
    expiresOn: "2026-02-01",
    cooldownUntil: null,
    cooldownGenericUntil: null,
    cooldown5hUntil: null,
    cooldownWeekUntil: null,
    cooldownMonthUntil: null,
    cooldownFreeUntil: null,
    lastError: null,
    authError: null,
    notes: null,
    usageSyncLastSuccessAt: null,
    usageSyncNextAllowedAt: null,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
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

test("Zen Free provider settings reject non-Zen accounts before the dedicated write", async () => {
  setActivePinia(createPinia());
  useControlPlaneStore().sync({ revision: 12, processGeneration: 42, pricingRevision: "p1" });
  const requests = installFetch(({ url }) => {
    if (url.endsWith("/accounts/go-account-2")) {
      return { id: "go-account-2", providerId: "opencode", revision: 12, processGeneration: 42 };
    }
    throw new Error(`unexpected request ${url}`);
  });

  await assert.rejects(
    () => providerApi.updateProviderSettings("go-account-2", { enabled: false }),
    /only Zen Free has provider settings/,
  );
  assert.deepEqual(requests.map(({ method, url }) => ({ method, url })), [
    { method: "GET", url: "/dashboard/api/v3/accounts/go-account-2" },
  ]);
});

test("Zen Free enable switch writes the catalog provider through PATCH /providers/zen-free", async () => {
  setActivePinia(createPinia());
  useControlPlaneStore().sync({ revision: 12, processGeneration: 42, pricingRevision: "p1" });
  let enabled = true;
  const requests = installFetch(({ url, method }) => {
    if (url.endsWith(`/accounts/${ZEN_FREE_ACCOUNT_ID}`)) {
      return zenFreeAccountDto({ enabled, revision: enabled ? 12 : 13 });
    }
    if (url.endsWith("/providers/zen-free") && method === "PATCH") {
      enabled = false;
      return {
        accountId: ZEN_FREE_ACCOUNT_ID,
        enabled: false,
        revision: 13,
        processGeneration: 42,
        pricingRevision: "p1",
      };
    }
    throw new Error(`unexpected request ${url}`);
  });

  const result = await providerApi.updateProviderSettings(ZEN_FREE_ACCOUNT_ID, { enabled: false });

  assert.equal(result.account.provider_id, "opencode-zen-free");
  assert.equal(result.account.enabled, false);
  assert.equal(result.revision, 13);
  assert.deepEqual(requests[1], {
    url: "/dashboard/api/v3/providers/zen-free",
    method: "PATCH",
    body: {
      enabled: false,
      expectedRevision: 12,
      processGeneration: 42,
    },
  });
});
