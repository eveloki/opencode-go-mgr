import assert from "node:assert/strict";
import test from "node:test";
import { readdir, readFile } from "node:fs/promises";
import { createPinia, setActivePinia } from "pinia";
import { dashboardApi } from "./dashboard.ts";
import {
  DASHBOARD_AUTH_REQUIRED_EVENT,
  DASHBOARD_GONE_EVENT,
  DashboardConflictError,
  DashboardGoneError,
  dashboardV3,
} from "./dashboard-v3.ts";
import { useConnectionStore } from "../stores/connection.ts";
import { useControlPlaneStore } from "../stores/controlPlane.ts";
import { useSessionStore } from "../stores/session.ts";

interface RecordedRequest {
  url: string;
  method: string;
  body: Record<string, unknown> | null;
}

function installBrowser(
  responder: (request: RecordedRequest) => Response | object,
  onEvent?: (event: { type: string; detail?: unknown }) => void,
): RecordedRequest[] {
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: {
      location: { pathname: "/dashboard" },
      dispatchEvent(event: { type: string; detail?: unknown }) {
        onEvent?.(event);
        return true;
      },
    },
  });
  if (!("CustomEvent" in globalThis)) {
    Object.defineProperty(globalThis, "CustomEvent", {
      configurable: true,
      value: class CustomEvent {
        readonly type: string;
        readonly detail: unknown;
        constructor(type: string, init?: { detail?: unknown }) {
          this.type = type;
          this.detail = init?.detail;
        }
      },
    });
  }
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

function pricingSnapshot(revision = 17, pricingRevision = "price-7") {
  return {
    revision,
    processGeneration: 99,
    pricingRevision,
    activatedAt: "2026-08-23T00:00:00Z",
    documentUpdatedAt: "2026-08-22",
    sourceUrl: "https://opencode.ai/docs/go/",
    contentHash: "hash",
    adjustmentPolicyVersion: "v1",
    limits: { window5h: 5, windowWeek: 7, windowMonth: 30 },
    models: [{
      modelId: "gpt-5.6-luna",
      displayName: "Luna",
      input: 1,
      output: 2,
      cacheRead: 0.1,
      cacheWrite: null,
      usage: 3,
      quotaMultiplier: 1,
      minInputTokens: null,
      maxInputTokens: null,
      timeWindow: "always",
      adjustments: [{ label: "Base", multiplier: 1, appliesTo: "all" }],
    }],
  };
}

function connection(revision: number, subKeys: Array<{ id: string; name: string; enabled: boolean; value: string }> = []) {
  return {
    revision,
    processGeneration: 99,
    gatewayPort: 9042,
    clientRootUrl: "http://127.0.0.1:9042",
    upstreamBaseUrl: "https://opencode.ai/zen/go",
    primaryKey: "ocg-primary-secret",
    subKeys,
  };
}

test("production code has exactly six stores and no dependency on the V2 endpoint module", async () => {
  const stores = (await readdir(new URL("../stores/", import.meta.url)))
    .filter((name) => name.endsWith(".ts"))
    .sort();
  assert.deepEqual(stores, [
    "accounts.ts",
    "connection.ts",
    "controlPlane.ts",
    "providers.ts",
    "session.ts",
    "settings.ts",
  ]);
  for (const relative of ["./dashboard.ts", "./providers.ts", "../stores/", "../views/"]) {
    const url = new URL(relative, import.meta.url);
    if (relative.endsWith("/")) continue;
    const source = await readFile(url, "utf8");
    assert.doesNotMatch(source, /from\s+["'][^"']*tauri(?:\.ts)?["']|\btauriApi\b|LegacyTauriApi/);
  }
});

test("read projections unwrap models, items, pricing limits, usage windows, and nulls", async () => {
  installBrowser(({ url }) => {
    if (url.endsWith("/application-models")) {
      return { revision: 2, processGeneration: 99, pricingRevision: "price-7", models: ["gpt-5.6-luna"] };
    }
    if (url.includes("/dashboard/daily-cost-by-model")) {
      return {
        revision: 2,
        processGeneration: 99,
        pricingRevision: "price-7",
        items: [{ date: "2026-08-23", model: "gpt-5.6-luna", cost: 1.25 }],
      };
    }
    if (url.endsWith("/pricing")) return pricingSnapshot(2);
    if (url.endsWith("/accounts/account-1/usage")) {
      return {
        revision: 2,
        processGeneration: 99,
        pricingRevision: null,
        accountId: "account-1",
        window5h: 4,
        windowWeek: 6,
        windowMonth: 8,
        resetsIn5h: null,
        resetsInWeek: "2026-08-24T00:00:00Z",
        resetsInMonth: null,
      };
    }
    throw new Error(`unexpected request ${url}`);
  });

  assert.deepEqual(await dashboardApi.getApplicationModels(), ["gpt-5.6-luna"]);
  assert.deepEqual(await dashboardApi.getDailyCostByModel(30), [
    { date: "2026-08-23", model: "gpt-5.6-luna", cost: 1.25 },
  ]);
  const pricing = await dashboardApi.getPricing();
  assert.deepEqual(pricing.limits, { window_5h: 5, window_week: 7, window_month: 30 });
  assert.equal(pricing.models[0]?.cache_write, null);
  assert.equal(pricing.models[0]?.adjustments[0]?.applies_to, "all");
  const usage = await dashboardApi.getAccountUsage("account-1");
  assert.deepEqual(usage, {
    account_id: "account-1",
    window_5h: 4,
    window_week: 6,
    window_month: 8,
    resets_in_5h: null,
    resets_in_week: "2026-08-24T00:00:00Z",
    resets_in_month: null,
  });
});

test("pricing refresh and multiplier writes send the pricing token separately from control CAS", async () => {
  setActivePinia(createPinia());
  const controlPlane = useControlPlaneStore();
  controlPlane.sync({ revision: 17, processGeneration: 99, pricingRevision: "price-7" });
  const requests = installBrowser(({ url }) => {
    if (url.endsWith("/pricing/refresh")) {
      return {
        refreshStatus: "unchanged",
        officialContentHash: null,
        multiplierChanges: [],
        error: null,
        snapshot: pricingSnapshot(18, "price-8"),
      };
    }
    if (url.endsWith("/pricing/multipliers")) return pricingSnapshot(19, "price-9");
    throw new Error(`unexpected request ${url}`);
  });

  const refreshed = await dashboardApi.refreshPricing({ policy: "keep_current" });
  assert.equal(refreshed.revision, "price-8");
  assert.equal(refreshed.refresh_status, "unchanged");
  assert.equal(controlPlane.revision, 18);
  assert.equal(controlPlane.pricingRevision, "price-8");
  await dashboardApi.updatePricingMultipliers(controlPlane.pricingRevision!, [{ model_id: "gpt-5.6-luna", multiplier: 1.2 }]);

  assert.deepEqual(requests.map(({ url, method, body }) => ({ url, method, body })), [
    {
      url: "/dashboard/api/v3/pricing/refresh",
      method: "POST",
      body: {
        expectedPricingRevision: "price-7",
        policy: "keep_current",
        expectedRevision: 17,
        processGeneration: 99,
      },
    },
    {
      url: "/dashboard/api/v3/pricing/multipliers",
      method: "PUT",
      body: {
        expectedPricingRevision: "price-8",
        multipliers: [{ modelId: "gpt-5.6-luna", multiplier: 1.2 }],
        expectedRevision: 18,
        processGeneration: 99,
      },
    },
  ]);
});

test("connection store identifies the exact new and regenerated Key values", async () => {
  setActivePinia(createPinia());
  useControlPlaneStore().sync({ revision: 5, processGeneration: 99, pricingRevision: "price-7" });
  let current = connection(5, [{ id: "old", name: "Shared", enabled: true, value: "ocg-old" }]);
  const requests = installBrowser(({ url, method }) => {
    if (method === "GET" && url.endsWith("/connection")) return current;
    if (method === "POST" && url.endsWith("/keys")) {
      current = connection(6, [
        ...current.subKeys,
        { id: "new-id", name: "Shared", enabled: true, value: "ocg-new-value" },
      ]);
      return { revision: 6, processGeneration: 99 };
    }
    if (method === "POST" && url.endsWith("/keys/old/regenerate")) {
      current = connection(7, current.subKeys.map((key) => (
        key.id === "old" ? { ...key, value: "ocg-regenerated" } : key
      )));
      return { revision: 7, processGeneration: 99 };
    }
    throw new Error(`unexpected request ${method} ${url}`);
  });
  const store = useConnectionStore();
  const created = await store.createKey("Shared");
  const regenerated = await store.regenerateKey("old");

  assert.equal(created.id, "new-id");
  assert.equal(created.value, "ocg-new-value");
  assert.equal(regenerated.id, "old");
  assert.equal(regenerated.value, "ocg-regenerated");
  assert.deepEqual(requests.filter(({ method }) => method === "POST").map(({ body }) => body), [
    { name: "Shared", expectedRevision: 5, processGeneration: 99 },
    { expectedRevision: 6, processGeneration: 99 },
  ]);
});

test("401 clears connection plaintext synchronously through the session event", async () => {
  setActivePinia(createPinia());
  const connectionStore = useConnectionStore();
  const session = useSessionStore();
  let unauthorized = false;
  const events: string[] = [];
  installBrowser(({ url }) => {
    if (url.endsWith("/connection")) return connection(3);
    if (unauthorized) {
      return new Response(JSON.stringify({ code: "unauthorized", message: "expired" }), {
        status: 401,
        headers: { "Content-Type": "application/json" },
      });
    }
    return { revision: 3, processGeneration: 99 };
  }, (event) => {
    events.push(event.type);
    if (event.type === DASHBOARD_AUTH_REQUIRED_EVENT) session.handleAuthRequired();
  });
  await connectionStore.load();
  assert.equal(connectionStore.info?.primary_key, "ocg-primary-secret");
  unauthorized = true;
  await assert.rejects(() => dashboardV3.getSettings());
  assert.equal(connectionStore.info, null);
  assert.deepEqual(events, [DASHBOARD_AUTH_REQUIRED_EVENT]);
});

test("409 refreshes tokens without replaying the rejected mutation", async () => {
  setActivePinia(createPinia());
  const control = useControlPlaneStore();
  control.sync({ revision: 4, processGeneration: 99, pricingRevision: "price-7" });
  let writes = 0;
  const requests = installBrowser(({ url, method }) => {
    if (method === "PATCH" && url.endsWith("/keys/key-1")) {
      writes += 1;
      return new Response(JSON.stringify({
        code: "revision_conflict",
        message: "stale",
        currentRevision: 5,
        processGeneration: 99,
      }), { status: 409, headers: { "Content-Type": "application/json" } });
    }
    if (method === "GET" && url.endsWith("/contract")) {
      return { revision: 5, processGeneration: 99, pricingRevision: "price-7" };
    }
    throw new Error(`unexpected request ${method} ${url}`);
  });

  await assert.rejects(
    () => control.runMutation((expectation) => dashboardV3.updateKey("key-1", { enabled: false }, expectation)),
    (error) => error instanceof DashboardConflictError,
  );
  assert.equal(writes, 1);
  assert.equal(control.revision, 5);
  assert.deepEqual(requests.map(({ method, url }) => ({ method, url })), [
    { method: "PATCH", url: "/dashboard/api/v3/keys/key-1" },
    { method: "GET", url: "/dashboard/api/v3/contract" },
  ]);
});

test("410 dispatches structured refresh guidance", async () => {
  const events: Array<{ type: string; detail?: unknown }> = [];
  installBrowser(() => new Response(JSON.stringify({
    code: "gone",
    message: "old SPA",
    currentRevision: 9,
    processGeneration: 99,
  }), { status: 410, headers: { "Content-Type": "application/json" } }), (event) => events.push(event));

  await assert.rejects(() => dashboardV3.getSettings(), (error) => error instanceof DashboardGoneError);
  assert.equal(events[0]?.type, DASHBOARD_GONE_EVENT);
  assert.match(String((events[0]?.detail as { guidance?: string })?.guidance), /刷新页面/);
});
