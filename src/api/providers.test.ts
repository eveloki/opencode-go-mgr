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

test("provider settings PATCH sends both flags with the revision guard", async () => {
  const requests = mockDashboardFetch(() => ({
    account: { id: "zen" },
    revision: 12,
  }));

  const result = await providerApi.updateProviderSettings("zen", {
    enabled: false,
    free_alias_enabled: true,
    expected_revision: 11,
  });

  assert.equal(requests[0]?.url, "/dashboard/api/accounts/zen/provider-settings");
  assert.equal(requests[0]?.method, "PATCH");
  assert.deepEqual(requests[0]?.body, {
    enabled: false,
    free_alias_enabled: true,
    expected_revision: 11,
  });
  assert.equal(result.revision, 12);
  assert.equal(result.account.id, "zen");
});

test("zen toggles go through provider settings, never the generic account patch", () => {
  const accounts = readFileSync(new URL("../views/Accounts.vue", import.meta.url), "utf8");

  assert.match(accounts, /providerApi\.updateProviderSettings/);
  assert.match(accounts, /expected_revision: revision/);
  assert.match(accounts, /error\.status === 409/);
  assert.doesNotMatch(accounts, /setAccountFreeAlias/);
});
