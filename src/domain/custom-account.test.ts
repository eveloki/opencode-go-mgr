import assert from "node:assert/strict";
import test from "node:test";
import {
  CUSTOM_ENDPOINT_URL_ISSUE_KEYS,
  customAccountNeedsVerification,
  customEndpointUrlIssue,
  customInferenceEndpointPlaceholder,
  expandCustomModelCapabilities,
  isStandardCustomInferenceEndpoint,
  normalizeCustomCapabilities,
  CustomCapabilityError,
} from "./custom-account.ts";
import type { Account } from "../api/dashboard.ts";

test("trusted Endpoint validation permits LAN, localhost, and HTTP", () => {
  for (const endpoint of [
    "http://192.168.1.10:8080/v1/chat/completions",
    "http://localhost:3000/responses",
    "http://[::1]:8080/messages",
    "https://api.example.com/v1/messages",
  ]) assert.equal(customEndpointUrlIssue(endpoint), null, endpoint);
  assert.equal(customEndpointUrlIssue("ftp://api.example.com"), "not_http");
  assert.equal(customEndpointUrlIssue("https://user:pass@api.example.com"), "with_credentials");
  assert.equal(CUSTOM_ENDPOINT_URL_ISSUE_KEYS.empty, "请填写完整 Endpoint");
});

test("standard inference paths alone enable model discovery", () => {
  assert.equal(customInferenceEndpointPlaceholder("chat_completions"), "https://api.example.com/v1/chat/completions");
  assert.ok(isStandardCustomInferenceEndpoint("https://api.example.com/v1/chat/completions", "chat_completions"));
  assert.ok(isStandardCustomInferenceEndpoint("https://api.example.com/v1/responses/", "responses"));
  assert.ok(isStandardCustomInferenceEndpoint("https://api.example.com/v1/messages", "messages"));
  assert.ok(!isStandardCustomInferenceEndpoint("https://api.example.com/custom/infer", "messages"));
  assert.ok(!isStandardCustomInferenceEndpoint("https://api.example.com/v1/messages", "responses"));
});

test("one protocol expands each model once and rejects mismatched rows", () => {
  const rows = expandCustomModelCapabilities(["m1", "m2"], "messages");
  assert.deepEqual(rows, [
    { model_id: "m1", protocol: "messages" },
    { model_id: "m2", protocol: "messages" },
  ]);
  assert.deepEqual(normalizeCustomCapabilities(rows, "messages"), [
    { model_id: "m1", protocol: "messages", source: "manual" },
    { model_id: "m2", protocol: "messages", source: "manual" },
  ]);
  assert.throws(
    () => normalizeCustomCapabilities([{ model_id: "m", protocol: "responses" }], "messages"),
    (error) => error instanceof CustomCapabilityError && error.issue === "protocol_mismatch",
  );
});

test("verification state only applies to Custom accounts", () => {
  const account = { provider_id: "custom", offering_id: "api", verification_status: "pending" } as Pick<
    Account,
    "provider_id" | "offering_id" | "verification_status"
  >;
  assert.ok(customAccountNeedsVerification(account));
  assert.ok(!customAccountNeedsVerification({ ...account, verification_status: "verified" }));
});
