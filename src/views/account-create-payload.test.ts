import assert from "node:assert/strict";
import test from "node:test";
import type { ProviderCatalogEntry } from "../api/providers.ts";
import { PLAN_DEFINITIONS } from "./plans.ts";
import {
  accountCreatePayloadErrorKey,
  AccountCreatePayloadError,
  buildCreateAccountPayload,
} from "./account-create-payload.ts";
import type { AccountCreatePayloadErrorCode } from "./account-create-payload.ts";

function catalogEntry(
  provider_id: string,
  offering_id: string,
  extra: Partial<ProviderCatalogEntry> = {},
): ProviderCatalogEntry {
  return {
    provider_id,
    offering_id,
    display_name: `${provider_id} ${offering_id}`,
    display_family: provider_id,
    credential_kind: "api_key",
    quota_scope: "key",
    singleton: false,
    creation_availability: "available",
    verification_policy: "not_required",
    verification_runtime_availability: "optional",
    routable: true,
    managed_registration: false,
    pricing_availability: "available",
    usage_availability: "available",
    manual_usage_calibration: false,
    quota_unit: "usd",
    model_source: "builtin",
    auth_schemes: ["bearer"],
    upstream_protocols: ["chat_completions", "responses", "messages"],
    form_fields: [],
    model_aliases: [],
    ...extra,
  };
}

const goPlan = PLAN_DEFINITIONS.find((p) => p.id === "opencode-go")!;
const goatPlan = PLAN_DEFINITIONS.find((p) => p.id === "command-code-goat")!;
const scnetPlan = PLAN_DEFINITIONS.find((p) => p.id === "scnet")!;
const customPlan = PLAN_DEFINITIONS.find((p) => p.id === "custom-endpoint")!;

test("GOAT payload uses the goat offering and omits custom fields", () => {
  const payload = buildCreateAccountPayload(
    goatPlan,
    undefined,
    { name: "GOAT", key: "goat-key" },
    catalogEntry("command-code", "goat"),
  );
  assert.equal(payload.provider_id, "command-code");
  assert.equal(payload.offering_id, "goat");
  assert.equal(payload.name, "GOAT");
  assert.equal(payload.key, "goat-key");
  assert.equal(payload.custom_config, undefined);
  assert.deepEqual(payload.model_capabilities, undefined);
  assert.deepEqual(payload.acknowledgements, undefined);
});

test("SCNet payload includes the chosen tier and risk acknowledgement", () => {
  const entry = catalogEntry("scnet", "token-plan-standard", {
    risk_notice: {
      acknowledgement_id: "scnet-token-plan-restrictions",
      version: "2026-08-21",
      source_url: "https://www.scnet.cn/ac/openapi/doc/2.0/moduleapi/plans/token-plan.html",
      body: "Restrictions apply.",
      content_hash: "abc",
    },
  });
  const payload = buildCreateAccountPayload(
    scnetPlan,
    "token-plan-standard",
    { name: "SCNet", key: "sk-tp-live", acknowledgement_accepted: true },
    entry,
  );
  assert.equal(payload.provider_id, "scnet");
  assert.equal(payload.offering_id, "token-plan-standard");
  assert.equal(payload.key, "sk-tp-live");
  assert.deepEqual(payload.acknowledgements, [
    { acknowledgement_id: "scnet-token-plan-restrictions", version: "2026-08-21" },
  ]);
  assert.equal(payload.custom_config, undefined);
});

test("SCNet payload rejects a missing acknowledgement", () => {
  const entry = catalogEntry("scnet", "token-plan-basic", {
    risk_notice: {
      acknowledgement_id: "scnet-token-plan-restrictions",
      version: "2026-08-21",
      source_url: "https://www.scnet.cn/ac/openapi/doc/2.0/moduleapi/plans/token-plan.html",
      body: "Restrictions apply.",
      content_hash: "abc",
    },
  });
  assert.throws(
    () => buildCreateAccountPayload(
      scnetPlan,
      "token-plan-basic",
      { name: "SCNet", key: "sk-tp-live" },
      entry,
    ),
    AccountCreatePayloadError,
  );
});

test("Custom payload includes custom_config and model_capabilities", () => {
  const payload = buildCreateAccountPayload(
    customPlan,
    undefined,
    {
      name: "Custom",
      key: "custom-key",
      base_url: "https://api.example.com/v1",
      upstream_protocol: "chat_completions",
      auth_scheme: "x-api-key",
      model_capabilities: [{ model_id: "my-model", protocol: "chat_completions" }],
    },
    catalogEntry("custom", "api"),
  );
  assert.equal(payload.provider_id, "custom");
  assert.equal(payload.offering_id, "api");
  assert.deepEqual(payload.custom_config, {
    base_url: "https://api.example.com/v1",
    upstream_protocol: "chat_completions",
    auth_scheme: "x-api-key",
  });
  assert.deepEqual(payload.model_capabilities, [
    { model_id: "my-model", protocol: "chat_completions", source: "manual" },
  ]);
  assert.deepEqual(payload.acknowledgements, undefined);
});

test("Custom payload rejects missing custom fields", () => {
  assert.throws(
    () => buildCreateAccountPayload(
      customPlan,
      undefined,
      { name: "Custom", key: "custom-key" },
      catalogEntry("custom", "api"),
    ),
    AccountCreatePayloadError,
  );
  assert.throws(
    () => buildCreateAccountPayload(
      customPlan,
      undefined,
      {
        name: "Custom",
        key: "custom-key",
        base_url: "https://api.example.com/v1",
        upstream_protocol: "chat_completions",
        auth_scheme: "x-api-key",
        model_capabilities: [],
      },
      catalogEntry("custom", "api"),
    ),
    AccountCreatePayloadError,
  );
});

test("Non-custom plans reject custom_config and model_capabilities", () => {
  assert.throws(
    () => buildCreateAccountPayload(
      goatPlan,
      undefined,
      {
        name: "GOAT",
        key: "goat-key",
        base_url: "https://api.example.com/v1",
        upstream_protocol: "chat_completions",
        auth_scheme: "bearer",
      } as never,
      catalogEntry("command-code", "goat"),
    ),
    AccountCreatePayloadError,
  );
  assert.throws(
    () => buildCreateAccountPayload(
      goatPlan,
      undefined,
      {
        name: "GOAT",
        key: "goat-key",
        model_capabilities: [{ model_id: "x", protocol: "chat_completions" }],
      } as never,
      catalogEntry("command-code", "goat"),
    ),
    AccountCreatePayloadError,
  );
});

test("OpenCode Go payload keeps legacy behavior without acknowledgements", () => {
  const payload = buildCreateAccountPayload(
    goPlan,
    undefined,
    { name: "Go", key: "ocg-key", purchase_date: "2026-08-21", notes: "note" },
    catalogEntry("opencode", "go"),
  );
  assert.equal(payload.provider_id, "opencode");
  assert.equal(payload.offering_id, "go");
  assert.equal(payload.purchase_date, "2026-08-21");
  assert.equal(payload.notes, "note");
  assert.equal(payload.custom_config, undefined);
  assert.equal(payload.acknowledgements, undefined);
  assert.equal(payload.model_capabilities, undefined);
});

test("OpenCode Go import stays explicit and rejects blank credentials without a catalog", () => {
  const payload = buildCreateAccountPayload(
    goPlan,
    undefined,
    { name: "Go", key: "ocg-key" },
    undefined,
  );
  assert.equal(payload.provider_id, "opencode");
  assert.equal(payload.offering_id, "go");
  assert.equal(payload.key, "ocg-key");

  assert.throws(
    () => buildCreateAccountPayload(goPlan, undefined, { name: "Go", key: "   " }, undefined),
    (error) => (
      error instanceof AccountCreatePayloadError
      && error.code === "missing_key"
      && accountCreatePayloadErrorKey(error) === "请填写 API Key"
    ),
  );
});

test("payload validation exposes stable codes that map to localized message keys", () => {
  const cases: Array<{
    code: AccountCreatePayloadErrorCode;
    key: ReturnType<typeof accountCreatePayloadErrorKey>;
    run: () => unknown;
  }> = [
    {
      code: "missing_name",
      key: "名称不能为空",
      run: () => buildCreateAccountPayload(goPlan, undefined, { name: " ", key: "key" }, undefined),
    },
    {
      code: "missing_base_url",
      key: "请填写 Base URL",
      run: () => buildCreateAccountPayload(customPlan, undefined, { name: "Custom", key: "key" }, catalogEntry("custom", "api")),
    },
    {
      code: "risk_acknowledgement_required",
      key: "请阅读并同意条款",
      run: () => buildCreateAccountPayload(
        scnetPlan,
        "token-plan-basic",
        { name: "SCNet", key: "key" },
        catalogEntry("scnet", "token-plan-basic", {
          risk_notice: {
            acknowledgement_id: "terms",
            version: "1",
            source_url: "https://example.com/terms",
            body: "Terms",
            content_hash: "hash",
          },
        }),
      ),
    },
  ];

  for (const item of cases) {
    assert.throws(item.run, (error) => (
      error instanceof AccountCreatePayloadError
      && error.message === item.code
      && error.code === item.code
      && accountCreatePayloadErrorKey(error) === item.key
    ));
  }
  assert.equal(accountCreatePayloadErrorKey(new Error("internal English detail")), "账号创建失败，请重试");
});

test("Custom payload accepts administrator-trusted LAN, localhost, and metadata HTTP URLs", () => {
  const trusted = [
    "http://192.168.1.10:8080/v1",
    "http://10.0.0.2/openai",
    "http://localhost:3000",
    "http://127.0.0.1:11434/v1",
    "http://169.254.169.254/latest",
    "https://api.example.com/v1",
  ];
  for (const base_url of trusted) {
    const payload = buildCreateAccountPayload(
      customPlan,
      undefined,
      {
        name: "Custom",
        key: "custom-key",
        base_url,
        upstream_protocol: "chat_completions",
        auth_scheme: "bearer",
        model_capabilities: [{ model_id: "m", protocol: "chat_completions" }],
      },
      catalogEntry("custom", "api"),
    );
    assert.equal(payload.custom_config?.base_url, base_url);
  }
});

test("Custom payload rejects malformed, non-http(s), and credentialed base URLs", () => {
  const baseValues = {
    name: "Custom",
    key: "custom-key",
    upstream_protocol: "chat_completions" as const,
    auth_scheme: "bearer" as const,
    model_capabilities: [{ model_id: "m", protocol: "chat_completions" as const }],
  };
  const cases: Array<{ base_url: string; code: AccountCreatePayloadErrorCode }> = [
    { base_url: "not-a-url", code: "invalid_base_url" },
    { base_url: "ftp://api.example.com", code: "base_url_not_http" },
    { base_url: "https://user:pass@api.example.com", code: "base_url_with_credentials" },
  ];
  for (const { base_url, code } of cases) {
    assert.throws(
      () => buildCreateAccountPayload(
        customPlan,
        undefined,
        { ...baseValues, base_url },
        catalogEntry("custom", "api"),
      ),
      (error) => error instanceof AccountCreatePayloadError && error.code === code,
    );
  }
});

test("Custom payload rejects duplicate normalized IDs, backend ID limits, and protocol mismatches", () => {
  const values = {
    name: "Custom",
    key: "custom-key",
    base_url: "https://api.example.com/v1",
    upstream_protocol: "responses" as const,
    auth_scheme: "bearer" as const,
  };
  const cases: Array<{ capabilities: Array<{ model_id: string; protocol: "responses" | "messages" }>; code: AccountCreatePayloadErrorCode }> = [
    {
      capabilities: [
        { model_id: " model-a ", protocol: "responses" },
        { model_id: "model-a", protocol: "responses" },
      ],
      code: "duplicate_model_id",
    },
    {
      capabilities: [{ model_id: "a".repeat(201), protocol: "responses" }],
      code: "model_id_too_long",
    },
    {
      capabilities: [{ model_id: "model-a", protocol: "messages" }],
      code: "capability_protocol_mismatch",
    },
  ];
  for (const { capabilities, code } of cases) {
    assert.throws(
      () => buildCreateAccountPayload(
        customPlan,
        undefined,
        { ...values, model_capabilities: capabilities },
        catalogEntry("custom", "api"),
      ),
      (error) => error instanceof AccountCreatePayloadError && error.code === code,
    );
  }
});
