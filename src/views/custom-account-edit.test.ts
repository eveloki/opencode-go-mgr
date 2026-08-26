import assert from "node:assert/strict";
import test from "node:test";
import type { Account } from "../api/dashboard.ts";
import {
  applyCustomAccountEditPlan,
  CustomCapabilityError,
  executeCustomAccountEdit,
  planCustomAccountEdit,
} from "../domain/custom-account.ts";

function customAccount(overrides: Partial<Account> = {}): Account {
  return {
    id: "custom-1",
    name: "Custom",
    username: "",
    password: "",
    key: "key",
    enabled: false,
    account_type: "key",
    setup_step: "ready",
    provider_id: "custom",
    offering_id: "api",
    credential_kind: "api_key",
    quota_scope: "key",
    purchase_date: "",
    expires_on: "",
    cooldown_until: null,
    cooldown_generic_until: null,
    cooldown_5h_until: null,
    cooldown_week_until: null,
    cooldown_month_until: null,
    cooldown_free_until: null,
    last_error: null,
    auth_error: null,
    notes: "",
    usage_sync_last_success_at: null,
    usage_sync_next_allowed_at: null,
    created_at: "2026-08-21T00:00:00Z",
    updated_at: "2026-08-21T00:00:00Z",
    verification_status: "verified",
    connection_verified_at: "2026-08-21T00:00:00Z",
    verification_error: null,
    plan_routable: true,
    custom_config: {
      account_id: "custom-1",
      base_url: "https://api.example.com/v1",
      upstream_protocols: ["responses"],
      auth_scheme: "bearer",
      created_at: "2026-08-21T00:00:00Z",
      updated_at: "2026-08-21T00:00:00Z",
    },
    model_capabilities: [{
      account_id: "custom-1",
      model_id: "model-a",
      protocol: "responses",
      verified_at: null,
      source: "manual",
    }],
    ...overrides,
  };
}

async function recordedWrites(plan: ReturnType<typeof planCustomAccountEdit>) {
  const calls: string[] = [];
  await applyCustomAccountEditPlan(plan, {
    account: async () => { calls.push("account"); },
    customConfig: async () => { calls.push("custom-config"); },
    capabilities: async () => { calls.push("model-capabilities"); },
  });
  return calls;
}

test("verified Custom metadata and no-op edits never rewrite canonical connection sections", async () => {
  const account = customAccount();
  const metadata = planCustomAccountEdit(account, {
    name: "Renamed",
    notes: "metadata only",
    base_url: "https://api.example.com/v1",
    model_capabilities: [{ model_id: "model-a", protocol: "responses" }],
  });
  assert.deepEqual(await recordedWrites(metadata), ["account"]);
  assert.equal(account.verification_status, "verified");
  assert.equal(account.connection_verified_at, "2026-08-21T00:00:00Z");

  const noOp = planCustomAccountEdit(account, {
    name: "Custom",
    notes: "",
    base_url: "  https://api.example.com/v1  ",
    model_capabilities: [{ model_id: " model-a ", protocol: "responses" }],
  });
  assert.deepEqual(await recordedWrites(noOp), []);
});

test("Custom key replacement restores capabilities that the account PATCH clears", async () => {
  const account = customAccount();
  const plan = planCustomAccountEdit(account, {
    name: "Custom",
    notes: "",
    key: "replacement-key",
    base_url: "https://api.example.com/v1",
    model_capabilities: [{ model_id: "model-a", protocol: "responses" }],
  });
  const calls: string[] = [];
  let rewritten: unknown;
  await applyCustomAccountEditPlan(plan, {
    account: async () => { calls.push("account"); },
    customConfig: async () => { calls.push("custom-config"); },
    capabilities: async (capabilities) => {
      calls.push("model-capabilities");
      rewritten = capabilities;
    },
  });
  assert.deepEqual(calls, ["account", "model-capabilities"]);
  assert.deepEqual(rewritten, [{
    model_id: "model-a",
    protocol: "responses",
    source: "manual",
  }]);
});

test("Custom base URL comparison ignores equivalent host, port, and path spellings", async () => {
  const account = customAccount();
  for (const base_url of [
    "https://api.example.com/v1/",
    " HTTPS://API.EXAMPLE.COM:443/v1/// ",
  ]) {
    const plan = planCustomAccountEdit(account, {
      name: "Custom",
      notes: "",
      base_url,
      model_capabilities: [{ model_id: "model-a", protocol: "responses" }],
    });
    assert.deepEqual(await recordedWrites(plan), [], base_url);
  }

  const changed = planCustomAccountEdit(account, {
    name: "Custom",
    notes: "",
    base_url: " https://API.example.com:444/v1/ ",
    model_capabilities: [{ model_id: "model-a", protocol: "responses" }],
  });
  assert.equal(changed.customConfig?.base_url, "https://API.example.com:444/v1/");
  assert.deepEqual(await recordedWrites(changed), ["custom-config"]);
});

test("Custom edits only call the dedicated endpoint for the canonical section that changed", async () => {
  const account = customAccount();
  const config = planCustomAccountEdit(account, {
    name: "Custom",
    notes: "",
    base_url: "https://api.example.net/v1",
    model_capabilities: [{ model_id: "model-a", protocol: "responses" }],
  });
  assert.deepEqual(await recordedWrites(config), ["custom-config"]);

  const capabilities = planCustomAccountEdit(account, {
    name: "Custom",
    notes: "",
    base_url: "https://api.example.com/v1",
    model_capabilities: [{ model_id: "model-b", protocol: "responses" }],
  });
  assert.deepEqual(await recordedWrites(capabilities), ["model-capabilities"]);
});

test("invalid Custom capability edits are rejected before any account mutation", async () => {
  const account = customAccount();
  const calls: string[] = [];
  await assert.rejects(
    () => executeCustomAccountEdit(account, {
      name: "Renamed",
      notes: "would otherwise PATCH first",
      base_url: "https://api.example.com/v1",
      model_capabilities: [
        { model_id: " model-a ", protocol: "responses" },
        { model_id: "model-a", protocol: "responses" },
      ],
    }, {
      account: async () => { calls.push("account"); },
      customConfig: async () => { calls.push("custom-config"); },
      capabilities: async () => { calls.push("model-capabilities"); },
    }),
    (error) => error instanceof CustomCapabilityError && error.issue === "duplicate_model_id",
  );
  assert.deepEqual(calls, []);

  assert.throws(
    () => planCustomAccountEdit(account, {
      name: "Custom",
      base_url: "https://api.example.com/v1",
      model_capabilities: [{ model_id: "model-a", protocol: "messages" }],
    }),
    (error) => error instanceof CustomCapabilityError && error.issue === "protocol_mismatch",
  );
  assert.throws(
    () => planCustomAccountEdit(account, {
      name: "Custom",
      base_url: "https://api.example.com/v1",
      model_capabilities: [{ model_id: "a".repeat(201), protocol: "responses" }],
    }),
    (error) => error instanceof CustomCapabilityError && error.issue === "model_id_too_long",
  );
});

test("Custom protocol set and auth scheme edits go through the custom-config route with the new values", async () => {
  const account = customAccount();
  const plan = planCustomAccountEdit(account, {
    name: "Custom",
    notes: "",
    base_url: "https://api.example.com/v1",
    upstream_protocols: ["messages", "chat_completions"],
    auth_scheme: "x-api-key",
    model_capabilities: [
      { model_id: "model-a", protocol: "chat_completions" },
      { model_id: "model-a", protocol: "messages" },
    ],
  });
  assert.deepEqual(plan.customConfig, {
    base_url: "https://api.example.com/v1",
    upstream_protocols: ["chat_completions", "messages"],
    auth_scheme: "x-api-key",
  });
  assert.deepEqual(await recordedWrites(plan), ["custom-config", "model-capabilities"]);

  // Re-submitting the saved values unchanged rewrites nothing.
  const settled = customAccount({
    custom_config: {
      ...account.custom_config!,
      upstream_protocols: ["chat_completions", "messages"],
      auth_scheme: "x-api-key",
    },
    model_capabilities: [
      { account_id: "custom-1", model_id: "model-a", protocol: "chat_completions", verified_at: null, source: "manual" },
      { account_id: "custom-1", model_id: "model-a", protocol: "messages", verified_at: null, source: "manual" },
    ],
  });
  const noOp = planCustomAccountEdit(settled, {
    name: "Custom",
    notes: "",
    base_url: "https://api.example.com/v1",
    upstream_protocols: ["chat_completions", "messages"],
    auth_scheme: "x-api-key",
    model_capabilities: [
      { model_id: "model-a", protocol: "chat_completions" },
      { model_id: "model-a", protocol: "messages" },
    ],
  });
  assert.deepEqual(await recordedWrites(noOp), []);

  // An empty protocol set is rejected before any mutation.
  assert.throws(
    () => planCustomAccountEdit(account, {
      name: "Custom",
      base_url: "https://api.example.com/v1",
      upstream_protocols: [],
      model_capabilities: [{ model_id: "model-a", protocol: "responses" }],
    }),
    (error) => error instanceof CustomCapabilityError && error.issue === "protocol_mismatch",
  );
});
