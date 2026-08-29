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

test("Custom payload uses one API URL and expands every model to its protocol", () => {
  const payload = buildCreateAccountPayload(customPlan, undefined, {
    name: "Custom",
    key: "custom-key",
    endpoint_url: "https://api.example.com/v1/responses",
    upstream_protocol: "responses",
    model_capabilities: [{ model_id: "m1" }, { model_id: "m2" }],
  });
  assert.deepEqual(payload.custom_config, {
    endpoint_url: "https://api.example.com/v1/responses",
    upstream_protocol: "responses",
  });
  assert.deepEqual(payload.model_capabilities, [
    { model_id: "m1", protocol: "responses", source: "manual" },
    { model_id: "m2", protocol: "responses", source: "manual" },
  ]);
});

test("Custom payload rejects missing or malformed Endpoint fields", () => {
  const base = {
    name: "Custom",
    key: "custom-key",
    upstream_protocol: "chat_completions" as const,
    model_capabilities: [{ model_id: "m" }],
  };
  const cases: Array<{ endpoint_url?: string; code: AccountCreatePayloadErrorCode }> = [
    { code: "missing_endpoint_url" },
    { endpoint_url: "not-a-url", code: "invalid_endpoint_url" },
    { endpoint_url: "ftp://api.example.com", code: "endpoint_url_not_http" },
    { endpoint_url: "https://user:pass@api.example.com", code: "endpoint_url_with_credentials" },
  ];
  for (const { endpoint_url, code } of cases) {
    assert.throws(
      () => buildCreateAccountPayload(customPlan, undefined, { ...base, endpoint_url }),
      (error) => error instanceof AccountCreatePayloadError && error.code === code,
    );
  }
});

test("Custom payload requires one upstream protocol and valid model IDs", () => {
  const base = {
    name: "Custom",
    key: "custom-key",
    endpoint_url: "http://localhost:3000/v1/messages",
    model_capabilities: [{ model_id: "m" }],
  };
  assert.throws(
    () => buildCreateAccountPayload(customPlan, undefined, base),
    (error) => error instanceof AccountCreatePayloadError && error.code === "missing_upstream_protocol",
  );
  assert.throws(
    () => buildCreateAccountPayload(customPlan, undefined, {
      ...base,
      upstream_protocol: "messages",
      model_capabilities: [{ model_id: " model-a " }, { model_id: "model-a" }],
    }),
    (error) => error instanceof AccountCreatePayloadError && error.code === "duplicate_model_id",
  );
});

test("non-Custom plans reject Custom-only fields", () => {
  assert.throws(
    () => buildCreateAccountPayload(goatPlan, undefined, {
      name: "GOAT",
      key: "key",
      endpoint_url: "https://api.example.com/v1/messages",
      upstream_protocol: "messages",
    }),
    (error) => error instanceof AccountCreatePayloadError && error.code === "custom_fields_not_allowed",
  );
  const payload = buildCreateAccountPayload(goPlan, undefined, { name: "Go", key: "key" });
  assert.equal(payload.custom_config, undefined);
  assert.equal(payload.model_capabilities, undefined);
});

test("payload error messages remain usable without a legacy config vocabulary", () => {
  assert.equal(
    accountCreatePayloadErrorKey(new AccountCreatePayloadError("missing_endpoint_url")),
    "请填写 API 地址",
  );
  assert.equal(accountCreatePayloadErrorKey(new Error("internal")), "账号创建失败，请重试");
});
