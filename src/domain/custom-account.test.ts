import assert from "node:assert/strict";
import test from "node:test";
import {
  CUSTOM_BASE_URL_ISSUE_KEYS,
  canonicalCustomProtocols,
  customAccountNeedsVerification,
  customBaseUrlIssue,
  expandCustomModelCapabilities,
  isCustomApiAccount,
  normalizeCustomCapabilities,
  CustomCapabilityError,
} from "./custom-account.ts";
import type { Account } from "../api/dashboard.ts";

function customAccount(verification_status: Account["verification_status"]) {
  return { provider_id: "custom", offering_id: "api", verification_status };
}

test("administrator-trusted base URLs allow LAN, localhost, metadata, and plain HTTP", () => {
  const trusted = [
    "http://192.168.1.10:8080/v1",
    "http://10.0.0.2/openai",
    "http://localhost:3000",
    "http://127.0.0.1:11434/v1",
    "http://[::1]:8080/v1",
    "http://169.254.169.254/latest",
    "http://nas.lan:4000/v1/",
    "https://api.example.com/v1",
  ];
  for (const url of trusted) {
    assert.equal(customBaseUrlIssue(url), null, url);
    assert.equal(customBaseUrlIssue(`  ${url}  `), null, `trimmed ${url}`);
  }
});

test("base URL validation only rejects malformed, non-http(s), and credentialed input", () => {
  assert.equal(customBaseUrlIssue(""), "empty");
  assert.equal(customBaseUrlIssue("   "), "empty");
  assert.equal(customBaseUrlIssue("not-a-url"), "malformed");
  assert.equal(customBaseUrlIssue("api.example.com/v1"), "malformed");
  assert.equal(customBaseUrlIssue("//api.example.com/v1"), "malformed");
  assert.equal(customBaseUrlIssue("ftp://api.example.com/v1"), "not_http");
  assert.equal(customBaseUrlIssue("javascript:alert(1)"), "not_http");
  assert.equal(customBaseUrlIssue("ws://api.example.com"), "not_http");
  assert.equal(customBaseUrlIssue("https://user:pass@api.example.com"), "with_credentials");
  assert.equal(customBaseUrlIssue("http://user@192.168.1.10:8080"), "with_credentials");

  for (const issue of ["empty", "malformed", "not_http", "with_credentials"] as const) {
    assert.ok(CUSTOM_BASE_URL_ISSUE_KEYS[issue], issue);
  }
});

test("verification state only applies to Custom API accounts and never blocks enablement", () => {
  assert.ok(isCustomApiAccount({ provider_id: "custom", offering_id: "api" }));
  assert.ok(!isCustomApiAccount({ provider_id: "opencode", offering_id: "go" }));
  assert.ok(!isCustomApiAccount({ provider_id: "custom", offering_id: "other" }));

  assert.ok(customAccountNeedsVerification(customAccount("pending")));
  assert.ok(customAccountNeedsVerification(customAccount("failed")));
  assert.ok(!customAccountNeedsVerification(customAccount("verified")));
  assert.ok(!customAccountNeedsVerification(customAccount("not_required")));
  assert.ok(!customAccountNeedsVerification({
    provider_id: "command-code",
    offering_id: "goat",
    verification_status: "pending",
  }));
});

test("protocol sets are canonicalized and model IDs expand into model × protocol rows", () => {
  assert.deepEqual(canonicalCustomProtocols(["messages", "chat_completions", "messages"]), [
    "chat_completions",
    "messages",
  ]);
  assert.deepEqual(canonicalCustomProtocols([]), []);
  assert.deepEqual(expandCustomModelCapabilities(["m1", "m2"], ["messages", "chat_completions"]), [
    { model_id: "m1", protocol: "chat_completions" },
    { model_id: "m1", protocol: "messages" },
    { model_id: "m2", protocol: "chat_completions" },
    { model_id: "m2", protocol: "messages" },
  ]);
});

test("capability rows must belong to the declared protocol set; duplicates are per (model, protocol)", () => {
  const rows = expandCustomModelCapabilities(["model-a"], ["chat_completions", "messages"]);
  assert.deepEqual(normalizeCustomCapabilities(rows, ["chat_completions", "messages"]), [
    { model_id: "model-a", protocol: "chat_completions", source: "manual" },
    { model_id: "model-a", protocol: "messages", source: "manual" },
  ]);

  assert.throws(
    () => normalizeCustomCapabilities([{ model_id: "model-a", protocol: "responses" }], ["chat_completions"]),
    (error) => error instanceof CustomCapabilityError && error.issue === "protocol_mismatch",
  );
  assert.throws(
    () => normalizeCustomCapabilities(rows, []),
    (error) => error instanceof CustomCapabilityError && error.issue === "protocol_mismatch",
  );
  assert.throws(
    () => normalizeCustomCapabilities(
      [...rows, { model_id: " model-a ", protocol: "messages" }],
      ["chat_completions", "messages"],
    ),
    (error) => error instanceof CustomCapabilityError && error.issue === "duplicate_model_id",
  );
});
