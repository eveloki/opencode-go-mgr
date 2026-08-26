import assert from "node:assert/strict";
import test from "node:test";
import { PLAN_DEFINITIONS } from "./plans.ts";
import {
  accountCreatePayloadErrorKey,
  AccountCreatePayloadError,
  buildCreateAccountPayload,
} from "./account-create-payload.ts";
import type { AccountCreatePayloadErrorCode } from "./account-create-payload.ts";

const goPlan = PLAN_DEFINITIONS.find((p) => p.id === "opencode-go")!;
const goatPlan = PLAN_DEFINITIONS.find((p) => p.id === "command-code-goat")!;
const customPlan = PLAN_DEFINITIONS.find((p) => p.id === "custom-endpoint")!;

test("GOAT payload uses the goat offering and omits custom fields", () => {
  const payload = buildCreateAccountPayload(
    goatPlan,
    undefined,
    { name: "GOAT", key: "goat-key" },
  );
  assert.equal(payload.provider_id, "command-code");
  assert.equal(payload.offering_id, "goat");
  assert.equal(payload.name, "GOAT");
  assert.equal(payload.key, "goat-key");
  assert.equal(payload.custom_config, undefined);
  assert.deepEqual(payload.model_capabilities, undefined);
});

test("Custom payload includes custom_config and the model × protocol-set expansion", () => {
  const payload = buildCreateAccountPayload(
    customPlan,
    undefined,
    {
      name: "Custom",
      key: "custom-key",
      base_url: "https://api.example.com/v1",
      upstream_protocols: ["chat_completions"],
      auth_scheme: "x-api-key",
      model_capabilities: [{ model_id: "my-model" }],
    },
  );
  assert.equal(payload.provider_id, "custom");
  assert.equal(payload.offering_id, "api");
  assert.deepEqual(payload.custom_config, {
    base_url: "https://api.example.com/v1",
    upstream_protocols: ["chat_completions"],
    auth_scheme: "x-api-key",
  });
  assert.deepEqual(payload.model_capabilities, [
    { model_id: "my-model", protocol: "chat_completions", source: "manual" },
  ]);
});

test("Custom payload expands every model across the whole checked protocol set in canonical order", () => {
  const payload = buildCreateAccountPayload(
    customPlan,
    undefined,
    {
      name: "Custom",
      key: "custom-key",
      base_url: "https://api.example.com/v1",
      upstream_protocols: ["messages", "chat_completions"],
      auth_scheme: "bearer",
      model_capabilities: [{ model_id: "m1" }, { model_id: "m2" }],
    },
  );
  assert.deepEqual(payload.custom_config?.upstream_protocols, ["chat_completions", "messages"]);
  assert.deepEqual(payload.model_capabilities, [
    { model_id: "m1", protocol: "chat_completions", source: "manual" },
    { model_id: "m1", protocol: "messages", source: "manual" },
    { model_id: "m2", protocol: "chat_completions", source: "manual" },
    { model_id: "m2", protocol: "messages", source: "manual" },
  ]);
});

test("Custom payload rejects missing custom fields", () => {
  assert.throws(
    () => buildCreateAccountPayload(
      customPlan,
      undefined,
      { name: "Custom", key: "custom-key" },
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
        upstream_protocols: ["chat_completions"],
        auth_scheme: "x-api-key",
        model_capabilities: [],
      },
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
        upstream_protocols: ["chat_completions"],
        auth_scheme: "bearer",
      } as never,
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
        model_capabilities: [{ model_id: "x" }],
      } as never,
    ),
    AccountCreatePayloadError,
  );
});

test("OpenCode Go payload keeps legacy behavior", () => {
  const payload = buildCreateAccountPayload(
    goPlan,
    undefined,
    { name: "Go", key: "ocg-key", purchase_date: "2026-08-21", notes: "note" },
  );
  assert.equal(payload.provider_id, "opencode");
  assert.equal(payload.offering_id, "go");
  assert.equal(payload.purchase_date, "2026-08-21");
  assert.equal(payload.notes, "note");
  assert.equal(payload.custom_config, undefined);
  assert.equal(payload.model_capabilities, undefined);
});

test("OpenCode Go import stays explicit and rejects blank credentials", () => {
  const payload = buildCreateAccountPayload(
    goPlan,
    undefined,
    { name: "Go", key: "ocg-key" },
  );
  assert.equal(payload.provider_id, "opencode");
  assert.equal(payload.offering_id, "go");
  assert.equal(payload.key, "ocg-key");

  assert.throws(
    () => buildCreateAccountPayload(goPlan, undefined, { name: "Go", key: "   " }),
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
      run: () => buildCreateAccountPayload(goPlan, undefined, { name: " ", key: "key" }),
    },
    {
      code: "missing_base_url",
      key: "请填写 Base URL",
      run: () => buildCreateAccountPayload(customPlan, undefined, { name: "Custom", key: "key" }),
    },
    {
      code: "missing_upstream_protocol",
      key: "请至少选择一个上游协议",
      run: () => buildCreateAccountPayload(customPlan, undefined, {
        name: "Custom",
        key: "key",
        base_url: "https://api.example.com/v1",
        auth_scheme: "bearer",
        model_capabilities: [{ model_id: "m" }],
      }),
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
        upstream_protocols: ["chat_completions"],
        auth_scheme: "bearer",
        model_capabilities: [{ model_id: "m" }],
      },
    );
    assert.equal(payload.custom_config?.base_url, base_url);
  }
});

test("Custom payload rejects malformed, non-http(s), and credentialed base URLs", () => {
  const baseValues = {
    name: "Custom",
    key: "custom-key",
    upstream_protocols: ["chat_completions"] as ("chat_completions")[],
    auth_scheme: "bearer" as const,
    model_capabilities: [{ model_id: "m" }],
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
      ),
      (error) => error instanceof AccountCreatePayloadError && error.code === code,
    );
  }
});

test("Custom payload rejects duplicate normalized IDs and backend ID limits", () => {
  const values = {
    name: "Custom",
    key: "custom-key",
    base_url: "https://api.example.com/v1",
    upstream_protocols: ["responses", "messages"] as ("responses" | "messages")[],
    auth_scheme: "bearer" as const,
  };
  const cases: Array<{ capabilities: Array<{ model_id: string }>; code: AccountCreatePayloadErrorCode }> = [
    {
      capabilities: [
        { model_id: " model-a " },
        { model_id: "model-a" },
      ],
      code: "duplicate_model_id",
    },
    {
      capabilities: [{ model_id: "a".repeat(201) }],
      code: "model_id_too_long",
    },
  ];
  for (const { capabilities, code } of cases) {
    assert.throws(
      () => buildCreateAccountPayload(
        customPlan,
        undefined,
        { ...values, model_capabilities: capabilities },
      ),
      (error) => error instanceof AccountCreatePayloadError && error.code === code,
    );
  }
});
