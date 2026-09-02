import assert from "node:assert/strict";
import test from "node:test";
import { dashboardApi } from "../api/dashboard.ts";
import { installFetchMock } from "../test-helpers/dashboard-v3-fetch.ts";

test("forward log API sends the provider attribution filters as exact query params", async () => {
  const requests = installFetchMock(() => ({
    revision: 1,
    processGeneration: 99,
    pricingRevision: null,
    items: [],
    summary: {
      totalRequests: 0,
      promptTokens: 0,
      completionTokens: 0,
      cachedTokens: 0,
      cost: 0,
    },
  }));

  await dashboardApi.getForwardLogs({
    limit: 20,
    offset: 40,
    provider_id: "opencode",
    offering_id: "go",
    route_account_id: "route 1",
    credential_account_id: "cred 2",
  });

  const query = new URL(requests[0]!.url, "http://localhost").searchParams;
  assert.equal(query.get("providerId"), "opencode");
  assert.equal(query.get("offeringId"), "go");
  assert.equal(query.get("routeAccountId"), "route 1");
  assert.equal(query.get("credentialAccountId"), "cred 2");
  assert.equal(query.get("limit"), "20");
  assert.equal(query.get("offset"), "40");
});
