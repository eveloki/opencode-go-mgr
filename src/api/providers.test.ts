import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { providerApi } from "./providers.ts";

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

test("provider catalog and offering pricing use the provider-scoped routes", async () => {
  const requests = mockDashboardFetch((url) => url.endsWith("/providers/catalog") ? [] : {
    provider_id: "command-code",
    offering_id: "goat",
    availability: "unconfigured",
  });

  await providerApi.getProviderCatalog();
  await providerApi.getProviderPricing("command-code", "goat");

  assert.equal(requests[0]?.url, "/dashboard/api/providers/catalog");
  assert.equal(requests[0]?.method, "GET");
  assert.equal(requests[1]?.url, "/dashboard/api/providers/command-code/goat/pricing");
  assert.equal(requests[1]?.method, "GET");
});

test("provider model capabilities expose concrete test protocols", async () => {
  const requests = mockDashboardFetch(() => ([{
    model_id: "gpt-5.6-luna",
    provider_id: "opencode",
    offering_id: "go",
    preferred_protocol: "responses",
    supported_protocols: ["chat_completions", "responses"],
  }]));

  const capabilities = await providerApi.getProviderModelCapabilities();

  assert.equal(requests[0]?.url, "/dashboard/api/providers/model-capabilities");
  assert.equal(requests[0]?.method, "GET");
  assert.deepEqual(capabilities[0]?.supported_protocols, ["chat_completions", "responses"]);
});

test("provider usage is read per account without a request body", async () => {
  const requests = mockDashboardFetch(() => ({
    account_id: "account-1",
    provider_id: "opencode",
    offering_id: "go",
    availability: "available",
    quota_windows: [],
    credit_balances: [],
    sync_state: null,
  }));

  const usage = await providerApi.getProviderUsage("account-1");

  assert.equal(requests[0]?.url, "/dashboard/api/accounts/account-1/provider-usage");
  assert.equal(requests[0]?.method, "GET");
  assert.equal(requests[0]?.body, null);
  assert.equal(usage.account_id, "account-1");
});

test("provider settings PATCH sends the enabled state with the revision guard", async () => {
  const requests = mockDashboardFetch(() => ({
    account: { id: "zen" },
    revision: 12,
  }));

  const result = await providerApi.updateProviderSettings("zen", {
    enabled: false,
    expected_revision: 11,
  });

  assert.equal(requests[0]?.url, "/dashboard/api/accounts/zen/provider-settings");
  assert.equal(requests[0]?.method, "PATCH");
  assert.deepEqual(requests[0]?.body, {
    enabled: false,
    expected_revision: 11,
  });
  assert.equal(result.revision, 12);
  assert.equal(result.account.id, "zen");
});

test("zen toggles go through provider settings, never the generic account patch", () => {
  const accounts = readFileSync(new URL("../views/Accounts.vue", import.meta.url), "utf8");

  assert.match(accounts, /providerApi\.updateProviderSettings/);
  assert.match(accounts, /expected_revision: revision/);
  assert.match(accounts, /error\.status !== 409/);
  assert.doesNotMatch(accounts, /setAccountFreeAlias/);
});

test("Zen model catalog GET and refresh use the account-scoped routes", async () => {
  const requests = mockDashboardFetch(() => ({
    account_id: "zen",
    models: [{ model_id: "coder-free", alias: "coder" }],
    refreshed_at: null,
    source_url: "https://opencode.ai/zen/v1/models",
  }));
  await providerApi.getProviderModels("zen");
  const refreshed = await providerApi.refreshProviderModels("zen");
  assert.equal(requests[0]?.url, "/dashboard/api/accounts/zen/provider-models");
  assert.equal(requests[0]?.method, "GET");
  assert.equal(requests[1]?.url, "/dashboard/api/accounts/zen/provider-models/refresh");
  assert.equal(requests[1]?.method, "POST");
  assert.equal("models" in refreshed && Array.isArray(refreshed.models), true);
});

test("provider contracts GET is a local dashboard path with no body", async () => {
  const requests = mockDashboardFetch(() => ({
    revision: 4,
    providers: [],
    custom_endpoints: [],
  }));

  const response = await providerApi.getProviderContracts();

  assert.equal(requests[0]?.url, "/dashboard/api/provider-contracts");
  assert.equal(requests[0]?.method, "GET");
  assert.equal(requests[0]?.body, null);
  assert.equal(response.revision, 4);
});

test("protocol switch PUT sends the shared revision as expected_revision", async () => {
  const requests = mockDashboardFetch(() => ({
    revision: 8,
    providers: [],
    custom_endpoints: [],
  }));

  const response = await providerApi.updateProviderContractProtocol(
    "provider",
    "opencode",
    "chat_completions",
    { enabled: false, expected_revision: 7 },
  );

  assert.equal(
    requests[0]?.url,
    "/dashboard/api/provider-contracts/provider/opencode/protocols/chat_completions",
  );
  assert.equal(requests[0]?.method, "PUT");
  assert.deepEqual(requests[0]?.body, {
    enabled: false,
    expected_revision: 7,
  });
  assert.equal(response.revision, 8);
});

test("protocol probes POST unique protocols and return per-protocol results", async () => {
  const requests = mockDashboardFetch(() => ({
    account_id: "acc-1",
    model_id: "gpt-5.6-luna",
    results: [{ protocol: "responses", success: true, skipped: false, error: null }],
    contract: null,
  }));

  const response = await providerApi.runProtocolProbes("acc-1", {
    model_id: "gpt-5.6-luna",
    protocols: ["responses", "chat_completions"],
  });

  assert.equal(requests[0]?.url, "/dashboard/api/accounts/acc-1/protocol-probes");
  assert.equal(requests[0]?.method, "POST");
  assert.deepEqual(requests[0]?.body, {
    model_id: "gpt-5.6-luna",
    protocols: ["responses", "chat_completions"],
  });
  assert.equal(response.results[0]?.success, true);
});
